//! GT1 fase B1: TRASPASO desde la postura alcanzada por la persecucion espacial hasta el contacto
//! posicional calibrado de A. Ver `docs/tentacle-grasp-handoff.md`. Nucleo puro: no lo llaman `app`/`main`,
//! no escribe World, nave, camara, controlador ni persecucion; sin cierre organico (B2) ni preview (C).
//!
//! **H-HANDOFF-01**: primer cuadro = postura publicada (copia exacta); despues la punta axial va de la punta
//! alcanzada al destino A con `smoothstep`, recalculando el destino con la memoria ENTRANTE de cada cuadro
//! (la memoria A1 gira y cambia el destino calibrado); el ultimo cuadro es la solucion A publicada tal cual.
//! El solver espacial y la API A no cambian.
use crate::{
    math::{Mat4, Vec3},
    visual::{
        tentacle_grasp_transition::{ContactAdapter, ContactSolution},
        tentacle_live_pursuit::{MAX_PURSUIT_DT, MAX_REACH_OBJ},
        tentacle_spatial_pursuit::{SpatialMemory, SpatialPose, SpatialPursuit},
    },
};

/// Duracion CANDIDATA del traspaso (s) para el estudio manual; no es regla del juego.
pub const HANDOFF_CANDIDATE_DURATION_S: f64 = 1.0;

type V3 = [f64; 3];

/// Instantanea de lo que la persecucion PUBLICO: buffers del brazo de la capa compuesta y la pose del
/// mismo cuadro (punta pedida y memoria publicada).
#[derive(Clone, Debug, PartialEq)]
pub struct ReachedPose {
    vertices: Vec<V3>,
    normals: Vec<V3>,
    pose: SpatialPose,
}

impl ReachedPose {
    /// Copia el brazo publicado por `SpatialPursuit::step` (capa `composite` desde `body_prefix`).
    pub fn capture(pursuit: &SpatialPursuit) -> Result<Self, String> {
        let pose = pursuit
            .last()
            .and_then(|s| s.shaft)
            .ok_or("la persecucion no publico un brazo deformado (reposo o sin paso)")?;
        if pursuit.memory() != Some(pose.memory) {
            return Err("memoria publicada distinta de la del ultimo cuadro".into());
        }
        let composite = pursuit
            .composite()
            .ok_or("persecucion sin capa compuesta")?;
        let (nv, nn) = pursuit.body_prefix().ok_or("persecucion sin prefijo")?;
        let vertices = composite
            .vertices
            .get(nv..)
            .ok_or("capa sin brazo")?
            .to_vec();
        let normals = composite
            .normals
            .get(nn..)
            .ok_or("capa sin brazo")?
            .to_vec();
        if vertices
            .iter()
            .chain(&normals)
            .any(|p| !p.iter().all(|x| x.is_finite()))
        {
            return Err("brazo publicado no finito".into());
        }
        Ok(Self {
            vertices,
            normals,
            pose,
        })
    }
    pub fn vertices(&self) -> &[[f64; 3]] {
        &self.vertices
    }
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }
    pub fn pose(&self) -> SpatialPose {
        self.pose
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffPhase {
    /// Llevando la punta desde la postura alcanzada hacia el contacto calibrado.
    Aligning,
    /// Contacto A publicado; hold determinista.
    Contact,
}

/// Estado publicado del traspaso.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandoffStatus {
    pub phase: HandoffPhase,
    pub elapsed_s: f64,
    pub duration_s: f64,
    /// Fraccion suavizada del traspaso (0 en la postura alcanzada, 1 en el contacto).
    pub progress: f64,
    /// Punta axial pedida para los buffers publicados (OBJ).
    pub axial_target_obj: [f64; 3],
    /// Destino A calculado con la memoria entrante del ultimo cuadro publicado (OBJ); `None` en el
    /// primer cuadro (todavia no se ha calculado).
    pub destination_obj: Option<[f64; 3]>,
    /// Desplazamiento maximo de vertice del ultimo cambio publicado (OBJ).
    pub last_step_obj: f64,
    /// Solucion A publicada al llegar.
    pub contact: Option<ContactSolution>,
}

/// Traspaso de un solo uso con matrices de nave/cuerpo capturadas al pedirlo.
pub struct GraspHandoff<'a> {
    adapter: &'a ContactAdapter,
    ship: Mat4,
    body: Mat4,
    start_tip: V3,
    vertices: Vec<V3>,
    normals: Vec<V3>,
    pose: SpatialPose,
    status: HandoffStatus,
}

/// Igualdad BIT A BIT (NaN nunca coincide con nada, -0 != +0).
fn same_bits(a: &Mat4, b: &Mat4) -> bool {
    a.m.iter()
        .flatten()
        .zip(b.m.iter().flatten())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}

fn max_step(a: &[V3], b: &[V3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(p, q)| (Vec3::from_array(*p) - Vec3::from_array(*q)).length())
        .fold(0.0, f64::max)
}

impl<'a> GraspHandoff<'a> {
    /// Inicia el traspaso con las matrices de nave dibujada/cuerpo ACTUALES (capturadas) y una duracion
    /// explicita (s). El primer cuadro publicado es una COPIA de la postura alcanzada, tras comprobar que el
    /// rig del adaptador la reproduce BIT A BIT con su punta y memoria (mismo enlace anatomico). Con `Err`
    /// no se crea nada.
    pub fn start(
        adapter: &'a ContactAdapter,
        reached: &ReachedPose,
        ship_model_world: &Mat4,
        body: &Mat4,
        duration_s: f64,
    ) -> Result<Self, String> {
        if !(duration_s.is_finite() && duration_s > 0.0) {
            return Err(format!("duracion del traspaso invalida: {duration_s}"));
        }
        // Valida ambas matrices (semejanzas finitas) y que el destino exista.
        adapter.anchor().desired_obj(ship_model_world, body)?;
        let rig = adapter.rig();
        if reached.vertices.len() != rig.rest_vertices().len()
            || reached.normals.len() != rig.rest_normals().len()
        {
            return Err("postura alcanzada sin la topologia del adaptador".into());
        }
        let (mut v, mut n) = (reached.vertices.clone(), reached.normals.clone());
        rig.deform(
            reached.pose.tip_obj,
            Some(reached.pose.memory),
            &mut v,
            &mut n,
        )
        .map_err(|e| format!("postura alcanzada no reproducible: {e}"))?;
        if v != reached.vertices || n != reached.normals {
            return Err(
                "el rig del adaptador no reproduce la postura publicada (enlace anatomico ajeno); nada iniciado"
                    .into(),
            );
        }
        Ok(Self {
            adapter,
            ship: *ship_model_world,
            body: *body,
            start_tip: reached.pose.tip_obj,
            vertices: reached.vertices.clone(),
            normals: reached.normals.clone(),
            pose: reached.pose,
            status: HandoffStatus {
                phase: HandoffPhase::Aligning,
                elapsed_s: 0.0,
                duration_s,
                progress: 0.0,
                axial_target_obj: reached.pose.tip_obj,
                destination_obj: None,
                last_step_obj: 0.0,
                contact: None,
            },
        })
    }

    /// Avanza el reloj PROPIO del estudio `dt` s (finito, en `[0, MAX_PURSUIT_DT]`) con las MISMAS matrices
    /// capturadas (bit a bit). H-HANDOFF-01: `s = smoothstep(t / D)`; el destino A se recalcula con la
    /// memoria publicada (la entrante de este cuadro); antes de `D` se publica `deform(T0 + (A - T0) s,
    /// memoria)` y en `D` la solucion A tal cual (buffers, pose y memoria de A). `dt = 0` y el hold en
    /// contacto no recalculan nada. Todo se prepara en temporales: con `Err` nada cambia ni el reloj avanza.
    pub fn advance(
        &mut self,
        ship_model_world: &Mat4,
        body: &Mat4,
        dt: f64,
    ) -> Result<HandoffStatus, String> {
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}; nada publicado"
            ));
        }
        if !(same_bits(ship_model_world, &self.ship) && same_bits(body, &self.body)) {
            return Err(
                "transform de nave/cuerpo distinto del capturado al iniciar el traspaso (limite del estudio B1); nada publicado"
                    .into(),
            );
        }
        let duration = self.status.duration_s;
        let elapsed = (self.status.elapsed_s + dt).min(duration);
        if self.status.phase == HandoffPhase::Contact || elapsed == self.status.elapsed_s {
            return Ok(self.status);
        }
        let x = elapsed / duration;
        let progress = x * x * (3.0 - 2.0 * x);
        let rig = self.adapter.rig();
        let memory = self.pose.memory;
        let (mut cv, mut cn) = (self.vertices.clone(), self.normals.clone());
        let contact =
            self.adapter
                .solve(rig, &self.ship, &self.body, Some(memory), &mut cv, &mut cn)?;
        let done = elapsed >= duration;
        let (vertices, normals, pose, target) = if done {
            (cv, cn, contact.pose, contact.axial_target_obj)
        } else {
            let t0 = Vec3::from_array(self.start_tip);
            let target = t0 + (Vec3::from_array(contact.axial_target_obj) - t0) * progress;
            let r = (target - Vec3::from_array(rig.rest_loop().center)).length();
            if !(r >= rig.min_radius() * (1.0 - 1e-12) && r <= MAX_REACH_OBJ) {
                return Err(format!(
                    "punta intermedia a {r} OBJ del lazo: fuera de [{}, {MAX_REACH_OBJ}]; nada publicado",
                    rig.min_radius()
                ));
            }
            let target = [target.x, target.y, target.z];
            let (mut v, mut n) = (self.vertices.clone(), self.normals.clone());
            let pose = rig.deform(target, Some(memory), &mut v, &mut n)?;
            (v, n, pose, target)
        };
        if self
            .adapter
            .root()
            .iter()
            .any(|&i| vertices[i] != rig.rest_vertices()[i])
        {
            return Err("la raiz del tramo no quedo exacta; nada publicado".into());
        }
        let step = max_step(&vertices, &self.vertices);
        if !step.is_finite() {
            return Err("salto de malla no finito; nada publicado".into());
        }
        // Publicacion conjunta (sin operaciones falibles desde aqui).
        self.vertices = vertices;
        self.normals = normals;
        self.pose = pose;
        self.status = HandoffStatus {
            phase: if done {
                HandoffPhase::Contact
            } else {
                HandoffPhase::Aligning
            },
            elapsed_s: elapsed,
            duration_s: duration,
            progress: if done { 1.0 } else { progress },
            axial_target_obj: target,
            destination_obj: Some(contact.axial_target_obj),
            last_step_obj: step,
            contact: done.then_some(contact),
        };
        Ok(self.status)
    }
    pub fn vertices(&self) -> &[[f64; 3]] {
        &self.vertices
    }
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }
    pub fn pose(&self) -> SpatialPose {
        self.pose
    }
    pub fn memory(&self) -> SpatialMemory {
        self.pose.memory
    }
    pub fn status(&self) -> HandoffStatus {
        self.status
    }
    /// P1 (solo lectura): adaptador con el que se creo este traspaso, para comprobar identidad.
    pub fn adapter(&self) -> &'a ContactAdapter {
        self.adapter
    }
    /// P1 (solo lectura): matrices de nave dibujada y cuerpo capturadas en `start`.
    pub fn transforms(&self) -> (Mat4, Mat4) {
        (self.ship, self.body)
    }
}
