//! GT1 fase A: CONTACTO ANATOMICO del tentaculo espacial con la nave (adaptador puro, sin cierre). Ver
//! `docs/tentacle-grasp-transition.md`. No esta conectado al juego: no lo llaman `app`/`main`, no escribe
//! World, nave, camara ni la persecucion; solo lee matrices y datos aprobados y escribe los buffers dados.
//!
//! - **Ancla aprobada**: centroide de los 105 vertices grip de la pose aprobada en `grip_frame` (168) del
//!   clip, expresado en coordenadas del modelo de la nave con `ship_obj_matrix(grip_frame)`. Derivada de
//!   los datos del paquete, no escrita a mano. NO es "centroide = origen de la nave".
//! - **Contacto**: centroide de los mismos 105 vertices en la malla REAL publicada por
//!   `SpatialRig::deform` (OBJ de la criatura).
//! - **H-CONTACT-01**: el solver espacial no cambia; solo se calibra la punta axial que se le pide
//!   (Newton acotado sobre el contacto real) y se rechaza sin publicar si no converge o sale del dominio.
use crate::{
    math::{Mat4, Vec3},
    visual::{
        tentacle_growth_cpu::{GrowthPackage, GrowthVertex, VertexSource},
        tentacle_live_pursuit::{MAX_REACH_OBJ, similarity_inverse},
        tentacle_spatial_pursuit::{SpatialMemory, SpatialPose, SpatialRig},
    },
};

/// Tolerancia del contacto (OBJ), la misma de CONTACT02 del preflight.
pub const CONTACT_TOLERANCE_OBJ: f64 = 1e-5;
/// Tope de iteraciones de Newton de la calibracion del objetivo axial (CANDIDATO; d02: 3-6 bastan).
pub const CONTACT_MAX_ITERATIONS: usize = 16;
/// Precision a la que se detiene la busqueda (OBJ); la ACEPTACION es `CONTACT_TOLERANCE_OBJ`.
const CONTACT_PRECISION_OBJ: f64 = 1e-10;
/// Paso de las diferencias finitas del Jacobiano (OBJ).
const JACOBIAN_STEP_OBJ: f64 = 1e-6;
/// Mitades maximas de la busqueda lineal por iteracion.
const LINE_SEARCH_HALVINGS: usize = 12;

type V3 = [f64; 3];

fn v(a: V3) -> Vec3 {
    Vec3::from_array(a)
}
fn arr(a: Vec3) -> V3 {
    [a.x, a.y, a.z]
}

/// Aplica la inversa de una semejanza (filas de `similarity_inverse`) a un punto de mundo.
fn to_local(rows: &[Vec3; 3], m: &Mat4, p: Vec3) -> Vec3 {
    let rel = p - m.column(3);
    Vec3::new(rows[0].dot(rel), rows[1].dot(rel), rows[2].dot(rel))
}

fn centroid(points: &[V3], indices: &[usize]) -> Vec3 {
    let mut sum = Vec3::ZERO;
    for &i in indices {
        sum += v(points[i]);
    }
    sum * (1.0 / indices.len() as f64)
}

/// Ancla aprobada del contacto en coordenadas del MODELO de la nave (las de `ship_obj_matrix`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApprovedContactAnchor {
    grip_frame: usize,
    ship_local: V3,
}

impl ApprovedContactAnchor {
    /// Deriva el ancla del paquete: pose aprobada cruda del `grip_frame` del manifest (sin evaluar en
    /// una malla temporal), grupo grip del manifest y matriz de la nave del mismo cuadro.
    pub fn derive(package: &GrowthPackage) -> Result<Self, String> {
        let manifest = package.capture().manifest();
        let grip_frame = manifest.grip_frame();
        let positions = package.data().base_positions(grip_frame)?;
        let grip = manifest.grip();
        if grip.is_empty() || grip.iter().any(|&i| i >= positions.len()) {
            return Err("grupo grip del manifest vacio o fuera de la pose aprobada".into());
        }
        let contact = centroid(positions, grip);
        let ship = manifest.ship_obj_matrix(grip_frame)?;
        let (rows, _) = similarity_inverse(&ship)
            .map_err(|e| format!("matriz de la nave del cuadro {grip_frame}: {e}"))?;
        let local = to_local(&rows, &ship, contact);
        if !(contact.is_finite() && local.is_finite()) {
            return Err("ancla aprobada no finita".into());
        }
        Ok(Self {
            grip_frame,
            ship_local: arr(local),
        })
    }
    pub fn grip_frame(&self) -> usize {
        self.grip_frame
    }
    pub fn ship_local(&self) -> [f64; 3] {
        self.ship_local
    }
    /// Contacto deseado (OBJ de la criatura) para la nave DIBUJADA `ship_model_world`
    /// (`Ship::transform(SHIP_MODEL_YAW).matrix()`) y el transform de la criatura `body`.
    pub fn desired_obj(&self, ship_model_world: &Mat4, body: &Mat4) -> Result<[f64; 3], String> {
        similarity_inverse(ship_model_world).map_err(|e| format!("matriz de la nave: {e}"))?;
        let (rows, _) =
            similarity_inverse(body).map_err(|e| format!("transform del cuerpo: {e}"))?;
        let desired = to_local(
            &rows,
            body,
            ship_model_world.transform_point(v(self.ship_local)),
        );
        if !desired.is_finite() {
            return Err("contacto deseado no finito".into());
        }
        Ok(arr(desired))
    }
}

/// Resultado publicado de un contacto.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactSolution {
    /// Pose espacial publicada (su `memory` es la que el llamador pasaria al cuadro siguiente).
    pub pose: SpatialPose,
    /// Punta axial pedida a `SpatialRig::deform` (OBJ).
    pub axial_target_obj: [f64; 3],
    /// Ancla aprobada transportada por la nave y el cuerpo actuales (OBJ).
    pub desired_contact_obj: [f64; 3],
    /// Centroide grip de la malla PUBLICADA (OBJ).
    pub contact_obj: [f64; 3],
    pub contact_error_obj: f64,
    /// Error del contacto pidiendo la punta en el origen de la nave (solver raw, CONTACT02).
    pub raw_contact_error_obj: f64,
    pub iterations: usize,
    pub evaluations: usize,
    /// Esfera de la malla publicada (centro de la caja, radio maximo; OBJ).
    pub bounds_obj: ([f64; 3], f64),
}

/// Enlace de contacto poseido: rig + ancla + grupos grip/raiz de una sola topologia.
pub struct ContactAdapter {
    rig: SpatialRig,
    anchor: ApprovedContactAnchor,
    grip: Vec<usize>,
    root: Vec<usize>,
}

impl ContactAdapter {
    /// Valida que el grip de la topologia publicada es EXACTAMENTE el grip del manifest (vertices
    /// originales) y deriva el ancla. Construye su propio rig del MISMO `package.data()`.
    /// `arm_rest` son las posiciones originales del brazo de la captura: en el juego se obtienen
    /// del modelo real de World via `correspondence.arm_vertex()`, NO de `base_positions(1)`.
    pub fn new(package: &GrowthPackage, arm_rest: &[[f64; 3]]) -> Result<Self, String> {
        let anchor = ApprovedContactAnchor::derive(package)?;
        let topology = package.data().topology();
        let mut resolved = Vec::with_capacity(topology.grip().len());
        for &i in topology.grip() {
            match topology.vertices().get(i) {
                Some(GrowthVertex::Retained {
                    source: VertexSource::Orig { orig, .. },
                    ..
                }) => resolved.push(*orig),
                _ => return Err(format!("vertice grip {i} no es un original del brazo")),
            }
        }
        let mut authored = package.capture().manifest().grip().to_vec();
        resolved.sort_unstable();
        authored.sort_unstable();
        if resolved != authored {
            return Err("el grip de la topologia no coincide con el grip del manifest".into());
        }
        let n = package.data().template().vertices.len();
        let root = topology.root().to_vec();
        if root.is_empty() || root.iter().chain(topology.grip()).any(|&i| i >= n) {
            return Err("grupos raiz/grip fuera de la plantilla".into());
        }
        Ok(Self {
            rig: SpatialRig::new(package.data(), arm_rest)?,
            anchor,
            grip: topology.grip().to_vec(),
            root,
        })
    }
    /// Rig construido con la MISMA topologia que los indices cacheados. Solo lectura.
    pub fn rig(&self) -> &SpatialRig {
        &self.rig
    }
    pub fn anchor(&self) -> ApprovedContactAnchor {
        self.anchor
    }
    pub fn grip(&self) -> &[usize] {
        &self.grip
    }
    pub fn root(&self) -> &[usize] {
        &self.root
    }

    /// H-CONTACT-01: calibra la punta axial `T` pedida a `SpatialRig::deform` para que el centroide grip
    /// de la malla REAL coincida con el ancla aprobada transportada por la nave dibujada y el cuerpo
    /// actuales. Arranca en el origen de la nave (el solver raw) y aplica Newton con Jacobiano por
    /// diferencias finitas de la MISMA deformacion y busqueda lineal monotona, con iteraciones y
    /// evaluaciones acotadas. Publica solo si el error real <= `CONTACT_TOLERANCE_OBJ`, `T` queda en
    /// `[min_radius, MAX_REACH_OBJ]` (dominio del controlador) y la raiz queda exacta. Con `Err` los buffers
    /// no cambian. `memory` se usa tal cual en cada evaluacion (la decide el llamador).
    /// `rig` debe ser `self.rig()`: cualquier objeto independiente devuelve `Err` ANTES de evaluar
    /// o indexar, incluso si sus bytes, longitudes o clases coinciden. Se conserva el argumento
    /// como comprobacion explicita de identidad para llamadores que manejan varios enlaces.
    pub fn solve(
        &self,
        rig: &SpatialRig,
        ship_model_world: &Mat4,
        body: &Mat4,
        memory: Option<SpatialMemory>,
        vertices: &mut [[f64; 3]],
        normals: &mut [[f64; 3]],
    ) -> Result<ContactSolution, String> {
        // Identidad del objeto poseido, no equivalencia de tamanos, clases ni geometria.
        // No existe constructor que admita un rig independiente ni acceso mutable al poseido.
        if !std::ptr::eq(rig, &self.rig) {
            return Err("rig ajeno al enlace anatomico del adaptador; nada publicado".into());
        }
        if vertices.len() != rig.rest_vertices().len() || normals.len() != rig.rest_normals().len()
        {
            return Err("buffers sin la topologia del tramo".into());
        }
        let desired = v(self.anchor.desired_obj(ship_model_world, body)?);
        let (rows, _) =
            similarity_inverse(body).map_err(|e| format!("transform del cuerpo: {e}"))?;
        let raw = to_local(&rows, body, ship_model_world.column(3));
        let mut sv = vertices.to_vec();
        let mut sn = normals.to_vec();
        let mut evaluations = 0;
        let mut contact_at = |t: Vec3| -> Result<(SpatialPose, Vec3), String> {
            evaluations += 1;
            let pose = rig.deform(arr(t), memory, &mut sv, &mut sn)?;
            Ok((pose, centroid(&sv, &self.grip)))
        };
        let (_, raw_contact) = contact_at(raw)?;
        let raw_error = (raw_contact - desired).length();
        let (mut t, mut contact, mut error) = (raw, raw_contact, raw_error);
        let mut iterations = 0;
        while error > CONTACT_PRECISION_OBJ && iterations < CONTACT_MAX_ITERATIONS {
            // Jacobiano por diferencias finitas hacia adelante (columnas = derivadas por eje).
            let mut j = [Vec3::ZERO; 3];
            let mut jacobian = true;
            for (i, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                match contact_at(t + axis * JACOBIAN_STEP_OBJ) {
                    Ok((_, c)) => j[i] = (c - contact) * (1.0 / JACOBIAN_STEP_OBJ),
                    Err(_) => jacobian = false,
                }
            }
            let det = j[0].dot(j[1].cross(j[2]));
            if !(jacobian && det.is_finite() && det.abs() > 1e-12) {
                break;
            }
            let rhs = desired - contact;
            let dx = Vec3::new(
                rhs.dot(j[1].cross(j[2])) / det,
                j[0].dot(rhs.cross(j[2])) / det,
                j[0].dot(j[1].cross(rhs)) / det,
            );
            if !dx.is_finite() {
                break;
            }
            let mut step = 1.0;
            let mut accepted = None;
            for _ in 0..=LINE_SEARCH_HALVINGS {
                let candidate = t + dx * step;
                if let Ok((_, c)) = contact_at(candidate) {
                    let e = (c - desired).length();
                    if e < error {
                        accepted = Some((candidate, c, e));
                        break;
                    }
                }
                step *= 0.5;
            }
            let Some((candidate, c, e)) = accepted else {
                break;
            };
            (t, contact, error) = (candidate, c, e);
            iterations += 1;
        }
        // Publicacion: la malla del objetivo final se recalcula (misma aritmetica, mismos bits) y se
        // valida sobre lo que se va a publicar.
        let (pose, contact) = contact_at(t)?;
        let error = (contact - desired).length();
        if error.is_nan() || error > CONTACT_TOLERANCE_OBJ {
            return Err(format!(
                "contacto sin converger: error {error} OBJ (tolerancia {CONTACT_TOLERANCE_OBJ}) tras {iterations} iteraciones y {evaluations} evaluaciones; nada publicado"
            ));
        }
        let r = (t - v(rig.rest_loop().center)).length();
        if !(r >= rig.min_radius() * (1.0 - 1e-12) && r <= MAX_REACH_OBJ) {
            return Err(format!(
                "punta axial calibrada a {r} OBJ del lazo: fuera de [{}, {MAX_REACH_OBJ}]; nada publicado",
                rig.min_radius()
            ));
        }
        if self.root.iter().any(|&i| sv[i] != rig.rest_vertices()[i]) {
            return Err("la raiz del tramo no quedo exacta; nada publicado".into());
        }
        let bounds = bounds(&sv)?;
        vertices.copy_from_slice(&sv);
        normals.copy_from_slice(&sn);
        Ok(ContactSolution {
            pose,
            axial_target_obj: arr(t),
            desired_contact_obj: arr(desired),
            contact_obj: arr(contact),
            contact_error_obj: error,
            raw_contact_error_obj: raw_error,
            iterations,
            evaluations,
            bounds_obj: bounds,
        })
    }
}

fn bounds(points: &[V3]) -> Result<([f64; 3], f64), String> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in points {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let center: V3 = std::array::from_fn(|a| (lo[a] + hi[a]) * 0.5);
    let radius = points
        .iter()
        .map(|p| (v(*p) - v(center)).length())
        .fold(0.0, f64::max);
    if !(v(center).is_finite() && radius.is_finite()) {
        return Err("esfera del tramo no finita".into());
    }
    Ok((center, radius))
}
