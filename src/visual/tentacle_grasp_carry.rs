//! GT1-CARRY: captura EFECTIVA de la nave tras el Gripped REAL de la preview C aprobada y arrastre organico
//! hacia la criatura actual (preview aislada, opt-in; no la llaman `app`/`main`). Ver
//! `docs/tentacle-capture-carry.md`.
//!
//! - Antes del Gripped: delegacion EXACTA en `GraspPreview` (C aprobada): libertad, cancelaciones, arte.
//! - Bloqueo SOLO en el cuadro del evento Gripped vigente, con la trayectoria entera prevalidada: primer
//!   cuadro = copia exacta de buffers y pose de P1 168; la nave conserva su pose (bits) y pierde inercia.
//! - Arrastre H1 (medido en d02, contrato): la punta se retrae en RECTA hacia el centro del lazo de reposo
//!   hasta `r_end` (distancia grip-lazo autorada del cuadro 'final', nunca bajo `min_radius`), con la
//!   curva autorada de |nave-origen| de 'arrastre'..'final' tras el agarre autorado. La forma CERRADA 168
//!   se compone con la `compose`/`validate` de P1 (`HeldClosure`) y la memoria FIJA del bloqueo: la pose
//!   es funcion pura del progreso. La nave REAL sigue al distal con la relacion rigida aceptada.
//! - X suelta y reinicia C desde el World ACTUAL; O2 > 15 % suelta (observacion); Err atomico aparte del
//!   `abort` explicito (sin capa, nave libre).
// Bucles por indice sobre componentes xyz y cofactores: estilo, no logica.
#![allow(clippy::needless_range_loop)]

use crate::{
    app::{MAX_PHYSICS_DT, advance_perception_frame},
    assets::obj::Model,
    math::{Mat4, Vec3},
    scene::{
        ship::{Ship, ShipInput},
        world::{SHIP_MODEL_YAW, SolarBodyOverride, World},
    },
    visual::{
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, SolarKeys},
        tentacle_grasp_closure::{ClosureClip, HeldClosure},
        tentacle_grasp_preview::{
            ArmBuffers, GraspPreview, PreviewEvent, PreviewKeys, PreviewPhase, PreviewStatus,
            RESET_KEY, study_transforms,
        },
        tentacle_live_pursuit::{MAX_PURSUIT_DT, PURSUIT_ENGAGE_OXYGEN, copy_model, same_model},
        tentacle_spatial_pursuit::{SpatialMemory, SpatialPose},
    },
};

type V3 = [f64; 3];

/// Muestras de la trayectoria prevalidadas ANTES de bloquear (ademas del extremo).
pub const CARRY_PREFLIGHT_SAMPLES: usize = 32;
/// Marcadores del manifest que fijan los tiempos candidatos.
const HOLD_STAGE: &str = "agarre";
const DRAG_STAGE: &str = "arrastre";
const FINAL_STAGE: &str = "final";

fn stage_frame(clip: &ClosureClip, name: &str) -> Result<usize, String> {
    clip.package()
        .capture()
        .manifest()
        .stages()
        .iter()
        .find(|(_, s)| s == name)
        .map(|(f, _)| *f)
        .ok_or(format!("el manifest no tiene el marcador '{name}'"))
}

/// Plan del arrastre derivado de datos (manifest) y dominio (rig). Inmutable; se valida al crear.
#[derive(Clone, Debug, PartialEq)]
pub struct CarryPlan {
    /// Agarre autorado antes de arrastrar: `grip_frame` .. 'arrastre' (s).
    pub hold_s: f64,
    /// Arrastre autorado: 'arrastre' .. 'final' (s).
    pub drag_s: f64,
    /// Extremo de la punta (OBJ, desde el centro del lazo): `max(min_radius, r_authored)`.
    pub r_end_obj: f64,
    /// |centroide grip autorado del cuadro 'final' - centro del lazo| (OBJ).
    pub r_authored_obj: f64,
    pub min_radius_obj: f64,
    /// Centro del lazo de reposo del rig (OBJ).
    pub center_obj: V3,
    /// Cuadros 'agarre', 'arrastre', 'final' del manifest.
    pub stage_frames: [usize; 3],
    fps: f64,
    /// Progreso autorado en cada cuadro 'arrastre'..='final' (0 .. 1, monotono).
    curve: Vec<f64>,
}

impl CarryPlan {
    /// Lee los marcadores, la curva autorada de |nave-origen| (monotona exigida) y el grip autorado del
    /// cuadro 'final'. `Err` si los datos no sirven; nunca inventa valores.
    pub fn from_clip(clip: &ClosureClip) -> Result<Self, String> {
        let capture = clip.package().capture();
        let manifest = capture.manifest();
        let (hold, drag, fin) = (
            stage_frame(clip, HOLD_STAGE)?,
            stage_frame(clip, DRAG_STAGE)?,
            stage_frame(clip, FINAL_STAGE)?,
        );
        let grip = clip.grip_frame();
        if !(grip < hold && hold <= drag && drag < fin && fin <= capture.frames()) {
            return Err(format!(
                "marcadores del arrastre incompatibles: grip {grip}, agarre {hold}, arrastre {drag}, final {fin}"
            ));
        }
        let fps = capture.fps() as f64;
        let distance = |f: usize| -> Result<f64, String> {
            let m = manifest.ship_obj_matrix(f)?;
            let d = Vec3::new(m.m[0][3], m.m[1][3], m.m[2][3]).length();
            if d.is_finite() {
                Ok(d)
            } else {
                Err(format!("nave autorada no finita en el cuadro {f}"))
            }
        };
        let (d0, d1) = (distance(drag)?, distance(fin)?);
        if d0 <= d1 {
            return Err("el arrastre autorado no acerca la nave a la criatura".into());
        }
        let mut curve = Vec::with_capacity(fin - drag + 1);
        let mut previous = d0;
        for f in drag..=fin {
            let d = distance(f)?;
            if d > previous {
                return Err(format!(
                    "curva autorada no monotona en el cuadro {f}: {d} > {previous}"
                ));
            }
            previous = d;
            curve.push(((d0 - d) / (d0 - d1)).clamp(0.0, 1.0));
        }
        curve[0] = 0.0;
        *curve.last_mut().expect("curva no vacia") = 1.0;
        let rig = clip.adapter().rig();
        let center = Vec3::from_array(rig.rest_loop().center);
        let positions = clip.package().data().base_positions(fin)?;
        let ids = manifest.grip();
        if ids.is_empty() || ids.iter().any(|&i| i >= positions.len()) {
            return Err("grip del manifest vacio o fuera de la pose 'final'".into());
        }
        let mut sum = Vec3::ZERO;
        for &i in ids {
            sum += Vec3::from_array(positions[i]);
        }
        let authored = (sum * (1.0 / ids.len() as f64) - center).length();
        let min_radius = rig.min_radius();
        if !(authored.is_finite() && min_radius.is_finite() && center.is_finite()) {
            return Err("extremo autorado no finito".into());
        }
        Ok(Self {
            hold_s: (drag - grip) as f64 / fps,
            drag_s: (fin - drag) as f64 / fps,
            r_end_obj: authored.max(min_radius),
            r_authored_obj: authored,
            min_radius_obj: min_radius,
            center_obj: [center.x, center.y, center.z],
            stage_frames: [hold, drag, fin],
            fps,
            curve,
        })
    }
    /// Progreso (0..=1) a `elapsed_s` del bloqueo: 0 durante el agarre, la curva autorada con interpolacion
    /// LINEAL entre cuadros durante el arrastre, 1 despues. No finito o negativo: 0.
    pub fn progress(&self, elapsed_s: f64) -> f64 {
        if !(elapsed_s.is_finite() && elapsed_s > self.hold_s) {
            return 0.0;
        }
        let s = elapsed_s - self.hold_s;
        if s >= self.drag_s {
            return 1.0;
        }
        let x = s * self.fps;
        let k = (x.floor() as usize).min(self.curve.len() - 2);
        let (a, b) = (self.curve[k], self.curve[k + 1]);
        (a + (b - a) * (x - k as f64)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarryPhase {
    /// Sin bloqueo: la fase de la preview C.
    Free(PreviewPhase),
    /// Bloqueada en la pose del Gripped (agarre autorado).
    Captured,
    /// Arrastre en curso.
    Dragging,
    /// Extremo alcanzado; sujecion mantenida hasta soltar.
    Held,
    /// Soltada por O2: ultima malla como OBSERVACION, nave libre.
    Released,
    /// Abortada (escena invalida): sin capa propia, nave libre.
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseReason {
    ResetKey,
    OxygenAboveEngage,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CarryEvent {
    Preview(PreviewEvent),
    Locked,
    CarryRejected(String),
    DragStarted,
    HeldReached,
    Released(ReleaseReason),
    Aborted(String),
    RequestIgnored,
    ResetRejected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CarryStatus {
    pub phase: CarryPhase,
    /// Ultimo estado de la preview C (en el bloqueo, el del Gripped).
    pub preview: PreviewStatus,
    pub events: Vec<CarryEvent>,
    /// Cuadros procesados con exito.
    pub frame: u64,
    /// Segundos del reloj PROPIO desde el bloqueo.
    pub elapsed_s: f64,
    pub progress: f64,
    pub tip_radius_obj: Option<f64>,
    pub r0_obj: Option<f64>,
    pub r_end_obj: Option<f64>,
    /// Salto max del tramo respecto al bloqueo (OBJ, `validate` de P1).
    pub last_step_obj: f64,
    pub release: Option<ReleaseReason>,
    pub error: Option<String>,
}

/// Capa propia: COPIA de la capa compuesta del episodio (prefijo Body exacto + tramo).
struct Layer {
    // Identidades completas e inmutables, tambien durante la observacion Released.
    source_model: Model,
    source_body: Model,
    model: Model,
    prefix: (usize, usize),
    bounds: (Vec3, f64),
}

impl Layer {
    fn arm(&self) -> (&[V3], &[V3]) {
        let (nv, nn) = self.prefix;
        (&self.model.vertices[nv..], &self.model.normals[nn..])
    }
    fn view(&self) -> SolarBodyOverride<'_> {
        SolarBodyOverride {
            model: &self.model,
            extra_bounds_obj: self.bounds,
        }
    }
}

/// Esfera (centro AABB, radio maximo) que contiene TODOS los puntos; `None` si no es finita.
fn sphere(points: &[V3]) -> Option<(Vec3, f64)> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in points {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let center = Vec3::new(
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    );
    let mut radius: f64 = 0.0;
    for p in points {
        let d = (Vec3::from_array(*p) - center).length();
        if !d.is_finite() {
            return None;
        }
        radius = radius.max(d);
    }
    (center.is_finite() && radius.is_finite() && !points.is_empty()).then_some((center, radius))
}

fn same_bits(a: &Mat4, b: &Mat4) -> bool {
    a.m.iter()
        .flatten()
        .zip(b.m.iter().flatten())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}

/// Marco rigido del distal (OBJ): rotacion `end_rotation` (filas) y centro distal del rig.
fn distal_frame(p: &SpatialPose) -> Mat4 {
    let mut g = Mat4::IDENTITY;
    for r in 0..3 {
        g.m[r][..3].copy_from_slice(&p.end_rotation[r]);
        g.m[r][3] = p.distal_center[r];
    }
    g
}

/// Inversa afin (cofactores 3x3 + traslacion); `Err` si es singular o no finita.
fn affine_inverse(m: &Mat4) -> Result<Mat4, String> {
    let a = m.m;
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if !(det.is_finite() && det.abs() > 1e-300) || a[3] != [0.0, 0.0, 0.0, 1.0] {
        return Err("marco del agarre singular o no afin".into());
    }
    let mut r = Mat4::IDENTITY;
    for i in 0..3 {
        for j in 0..3 {
            let (i1, i2) = ((j + 1) % 3, (j + 2) % 3);
            let (j1, j2) = ((i + 1) % 3, (i + 2) % 3);
            r.m[i][j] = (a[i1][j1] * a[i2][j2] - a[i1][j2] * a[i2][j1]) / det;
        }
    }
    for i in 0..3 {
        r.m[i][3] = -(0..3).map(|k| r.m[i][k] * a[k][3]).sum::<f64>();
    }
    if r.m.iter().flatten().all(|x| x.is_finite()) {
        Ok(r)
    } else {
        Err("inversa del marco del agarre no finita".into())
    }
}

/// Pose de nave (posicion, orientacion de `Ship`) para la matriz de render `s` (modelo de la nave).
fn ship_pose_from_render(s: &Mat4) -> (Vec3, Mat4) {
    let mut rotation = Mat4::IDENTITY;
    for r in 0..3 {
        rotation.m[r][..3].copy_from_slice(&s.m[r][..3]);
    }
    (
        Vec3::new(s.m[0][3], s.m[1][3], s.m[2][3]),
        (rotation * Mat4::rotation_y(-SHIP_MODEL_YAW)).orthonormalized(),
    )
}

/// Nave libre con la MISMA pose y sin inercia (`Ship::new` + orientacion y ajustes); infalible.
fn free_ship(world: &mut World) {
    let mut ship = Ship::new(world.ship.position, -Vec3::Z);
    ship.orientation = world.ship.orientation;
    ship.settings = world.ship.settings;
    world.ship = ship;
}

/// Bloqueo vigente.
struct Lock<'c, 'p> {
    held: HeldClosure<'c, 'p>,
    memory: SpatialMemory,
    layer: Layer,
    body0: Mat4,
    ship0: (Vec3, Mat4),
    /// (B0 * G0)^-1 * S0: nave de render en el marco distal del bloqueo.
    rel: Mat4,
    center: Vec3,
    t0: Vec3,
    r0: f64,
    r_end: f64,
    elapsed: f64,
    progress: f64,
    tip: Vec3,
    drag_started: bool,
    held_reached: bool,
}

impl Lock<'_, '_> {
    fn tip_at(&self, q: f64) -> Vec3 {
        if self.r0 <= self.r_end || q <= 0.0 {
            return self.t0;
        }
        self.center + (self.t0 - self.center) * (1.0 - q * (1.0 - self.r_end / self.r0))
    }
}

enum Mode<'c, 'p> {
    Free,
    Locked(Box<Lock<'c, 'p>>),
    Released(Box<Layer>),
    Aborted,
}

/// Preview de captura y arrastre: posee la preview C; con `enabled = false` es la ruta legacy (C pura).
pub struct CaptureCarry<'c, 'p> {
    preview: GraspPreview<'c, 'p>,
    enabled: bool,
    plan: CarryPlan,
    mode: Mode<'c, 'p>,
    status: CarryStatus,
}

impl<'c, 'p> CaptureCarry<'c, 'p> {
    pub fn new(preview: GraspPreview<'c, 'p>, enabled: bool) -> Result<Self, String> {
        let plan = CarryPlan::from_clip(preview.clip())?;
        let status = CarryStatus {
            phase: CarryPhase::Free(preview.status().phase),
            preview: preview.status(),
            events: Vec::new(),
            frame: 0,
            elapsed_s: 0.0,
            progress: 0.0,
            tip_radius_obj: None,
            r0_obj: None,
            r_end_obj: None,
            last_step_obj: 0.0,
            release: None,
            error: None,
        };
        Ok(Self {
            preview,
            enabled,
            plan,
            mode: Mode::Free,
            status,
        })
    }

    /// Un cuadro DESPUES de la fisica del World (`dt` = el de la preview, en `[0, MAX_PURSUIT_DT]`). Con
    /// `Err` NADA cambia (estado, reloj, capa, nave, camara); el escape ante escena perdida es `abort`.
    pub fn frame(
        &mut self,
        world: &mut World,
        dt: f64,
        keys: PreviewKeys,
    ) -> Result<CarryStatus, String> {
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}; nada cambia"
            ));
        }
        // Antes de reset/request, relojes, estado o nave: cualquier modo con capa propia exige
        // la misma identidad COMPLETA y la composicion vigente, no solo prefijos o cantidades.
        if let Some(layer) = self.own_layer() {
            let model = world
                .creature()
                .ok_or("el World no tiene criatura")?
                .model();
            let body = world
                .staged_creature_parts()
                .ok_or("el World no tiene particion")?
                .body();
            if !same_model(&layer.source_model, model) || !same_model(&layer.source_body, body) {
                return Err("criatura o particion distintas del bloqueo; nada cambia".into());
            }
            world.validate_solar_body_override(&layer.view())?;
        }
        let mut events = Vec::new();
        match &self.mode {
            Mode::Free => self.free_frame(world, dt, keys, &mut events)?,
            Mode::Locked(_) => self.locked_frame(world, dt, keys, &mut events, false)?,
            Mode::Released(_) | Mode::Aborted => {
                if let Mode::Released(layer) = &self.mode {
                    // La observacion se dibuja: revalidarla contra el World vigente (sin render viejo).
                    world.validate_solar_body_override(&layer.view())?;
                }
                if keys.reset {
                    self.reset_preview(world, dt, &mut events)?;
                } else if keys.request {
                    events.push(CarryEvent::RequestIgnored);
                }
            }
        }
        self.status.events = events;
        self.status.phase = self.phase();
        self.status.frame += 1;
        Ok(self.status.clone())
    }

    pub(crate) fn validate_frozen(&self, world: &World, dt: f64) -> Result<(), String> {
        self.preview.validate_frozen(world, dt)?;
        if let Some(layer) = self.own_layer() {
            if !world
                .creature()
                .is_some_and(|c| same_model(&layer.source_model, c.model()))
                || !world
                    .staged_creature_parts()
                    .is_some_and(|p| same_model(&layer.source_body, p.body()))
            {
                return Err("criatura o particion distintas del bloqueo; nada cambia".into());
            }
            world.validate_solar_body_override(&layer.view())?;
        }
        Ok(())
    }

    /// Reaccion al O2 FINAL: sin integrar pose ni reloj de nuevo.
    pub(crate) fn react_to_oxygen(&mut self, world: &mut World) -> Result<(), String> {
        if world.debug_oxygen01 <= PURSUIT_ENGAGE_OXYGEN {
            return Ok(());
        }
        if let Mode::Locked(lock) = &self.mode {
            world.validate_solar_body_override(&lock.layer.view())?;
            free_ship(world);
            let Mode::Locked(lock) = std::mem::replace(&mut self.mode, Mode::Free) else {
                unreachable!()
            };
            self.mode = Mode::Released(Box::new(lock.layer));
            self.status.release = Some(ReleaseReason::OxygenAboveEngage);
            self.status
                .events
                .push(CarryEvent::Released(ReleaseReason::OxygenAboveEngage));
        } else if matches!(self.mode, Mode::Free) {
            let old = self.preview.status().events.len();
            self.preview.react_to_oxygen(world);
            let st = self.preview.status();
            self.status
                .events
                .extend(st.events[old..].iter().cloned().map(CarryEvent::Preview));
            self.status.preview = st;
        }
        self.status.phase = self.phase();
        Ok(())
    }

    /// Nuevo juego: conserva clip, plan y configuracion; descarta el episodio sin avance.
    pub(crate) fn restart_game(&mut self, world: &World) -> Result<(), String> {
        self.preview.restart_game(world)?;
        self.mode = Mode::Free;
        self.clear_lock_status();
        self.status.phase = CarryPhase::Free(self.preview.status().phase);
        self.status.preview = self.preview.status();
        self.status.events.clear();
        self.status.frame = 0;
        Ok(())
    }

    /// X fuera del bloqueo (observacion o abortada): reinicio de C desde el World actual.
    fn reset_preview(
        &mut self,
        world: &mut World,
        dt: f64,
        events: &mut Vec<CarryEvent>,
    ) -> Result<(), String> {
        let st = self.preview.frame(
            world,
            dt,
            PreviewKeys {
                request: false,
                reset: true,
            },
        )?;
        events.extend(st.events.iter().cloned().map(CarryEvent::Preview));
        if st.events.contains(&PreviewEvent::Reset) {
            self.mode = Mode::Free;
            self.clear_lock_status();
        }
        self.status.preview = st;
        Ok(())
    }

    fn clear_lock_status(&mut self) {
        let s = &mut self.status;
        (s.elapsed_s, s.progress, s.last_step_obj) = (0.0, 0.0, 0.0);
        (s.tip_radius_obj, s.r0_obj, s.r_end_obj) = (None, None, None);
        (s.release, s.error) = (None, None);
    }

    fn free_frame(
        &mut self,
        world: &mut World,
        dt: f64,
        keys: PreviewKeys,
        events: &mut Vec<CarryEvent>,
    ) -> Result<(), String> {
        let st = self.preview.frame(world, dt, keys)?;
        events.extend(st.events.iter().cloned().map(CarryEvent::Preview));
        if st.events.contains(&PreviewEvent::Reset) {
            self.clear_lock_status();
        }
        let gripped_now =
            st.events.contains(&PreviewEvent::Gripped) && st.phase == PreviewPhase::Gripped;
        self.status.preview = st;
        if self.enabled && gripped_now {
            match self.acquire(world) {
                Ok(lock) => {
                    self.status.elapsed_s = 0.0;
                    self.status.progress = 0.0;
                    self.status.last_step_obj = 0.0;
                    self.status.tip_radius_obj = Some(lock.r0);
                    self.status.r0_obj = Some(lock.r0);
                    self.status.r_end_obj = Some(lock.r_end.min(lock.r0));
                    self.status.release = None;
                    self.status.error = None;
                    self.mode = Mode::Locked(lock);
                    events.push(CarryEvent::Locked);
                }
                Err(e) => events.push(CarryEvent::CarryRejected(e)),
            }
        }
        Ok(())
    }

    /// Bloqueo en el cuadro del Gripped: instantanea de P1, capa copiada (comprobada contra la de C),
    /// trayectoria prevalidada y nave sin inercia con su pose EXACTA. Lo falible va antes del commit.
    fn acquire(&self, world: &mut World) -> Result<Box<Lock<'c, 'p>>, String> {
        let closure = self.preview.closure().ok_or("sin cierre P1 vivo")?;
        let held = closure.held()?;
        let pose = held.pose();
        let memory = pose.memory;
        let center = Vec3::from_array(self.plan.center_obj);
        let t0 = Vec3::from_array(pose.tip_obj);
        let r0 = (t0 - center).length();
        if !(r0.is_finite() && t0.is_finite()) {
            return Err("punta del agarre no finita".into());
        }
        let view = self
            .preview
            .body_override()
            .ok_or("C sin capa en el Gripped")?;
        let prefix = self
            .preview
            .pursuit()
            .body_prefix()
            .ok_or("persecucion sin prefijo")?;
        let layer = Layer {
            source_model: copy_model(world.creature().ok_or("sin criatura")?.model()),
            source_body: copy_model(world.staged_creature_parts().ok_or("sin particion")?.body()),
            model: copy_model(view.model),
            prefix,
            bounds: view.extra_bounds_obj,
        };
        let (v, n) = layer.arm();
        if v != held.vertices() || n != held.normals() {
            return Err("la capa de C no es la instantanea del cierre; sin bloqueo".into());
        }
        let mut lock = Lock {
            held,
            memory,
            layer,
            body0: Mat4::IDENTITY,
            ship0: (world.ship.position, world.ship.orientation),
            rel: Mat4::IDENTITY,
            center,
            t0,
            r0,
            r_end: self.plan.r_end_obj,
            elapsed: 0.0,
            progress: 0.0,
            tip: t0,
            drag_started: false,
            held_reached: false,
        };
        // Trayectoria ENTERA con la composicion/validacion de P1 antes de bloquear nada.
        for k in 1..=CARRY_PREFLIGHT_SAMPLES {
            let tip = lock.tip_at(k as f64 / CARRY_PREFLIGHT_SAMPLES as f64);
            lock.held
                .evaluate([tip.x, tip.y, tip.z], memory)
                .map_err(|e| format!("trayectoria no valida en la muestra {k}: {e}"))?;
        }
        let (ship, body) = study_transforms(world)?;
        lock.body0 = body;
        lock.rel = affine_inverse(&(body * distal_frame(&pose)))? * ship;
        if !lock.rel.m.iter().flatten().all(|x| x.is_finite()) {
            return Err("relacion de agarre no finita".into());
        }
        // Ultimo paso falible: la nave conserva su pose (bits) y pierde velocidad e inercia angular.
        world.commit_captured_ship_pose(lock.ship0.0, lock.ship0.1)?;
        Ok(Box::new(lock))
    }

    fn locked_frame(
        &mut self,
        world: &mut World,
        dt: f64,
        keys: PreviewKeys,
        events: &mut Vec<CarryEvent>,
        defer_oxygen: bool,
    ) -> Result<(), String> {
        let Mode::Locked(lock) = &self.mode else {
            return Err("sin bloqueo".into());
        };
        // Escena: modelo y particion del bloqueo, transforms finitos; la capa se revalida al publicar.
        let model = world
            .creature()
            .ok_or("el World no tiene criatura")?
            .model();
        let parts = world
            .staged_creature_parts()
            .ok_or("el World no tiene particion")?;
        if !same_model(&lock.layer.source_model, model)
            || !same_model(&lock.layer.source_body, parts.body())
        {
            return Err("criatura o particion distintas del bloqueo; nada cambia".into());
        }
        let (_, body) = study_transforms(world)?;
        if keys.reset {
            // X: C se reinicia desde el World actual (atomica); despues se suelta la nave sin teleport.
            let st = self.preview.frame(
                world,
                dt,
                PreviewKeys {
                    request: false,
                    reset: true,
                },
            )?;
            events.push(CarryEvent::Released(ReleaseReason::ResetKey));
            events.extend(st.events.iter().cloned().map(CarryEvent::Preview));
            free_ship(world);
            let layer = match std::mem::replace(&mut self.mode, Mode::Free) {
                Mode::Locked(l) => l.layer,
                _ => unreachable!("modo comprobado arriba"),
            };
            if st.events.contains(&PreviewEvent::Reset) {
                self.clear_lock_status();
            } else {
                // Reinicio de C rechazado: la nave queda libre y la ultima malla como observacion.
                let why = st
                    .error
                    .clone()
                    .unwrap_or_else(|| "reinicio rechazado".into());
                events.push(CarryEvent::ResetRejected(why));
                self.mode = Mode::Released(Box::new(layer));
            }
            self.status.release = Some(ReleaseReason::ResetKey);
            self.status.preview = st;
            return Ok(());
        }
        if keys.request {
            events.push(CarryEvent::RequestIgnored);
        }
        let o2 = world.debug_oxygen01;
        if !defer_oxygen && (o2.is_nan() || o2 > PURSUIT_ENGAGE_OXYGEN) {
            world.validate_solar_body_override(&lock.layer.view())?;
            free_ship(world);
            let layer = match std::mem::replace(&mut self.mode, Mode::Free) {
                Mode::Locked(l) => l.layer,
                _ => unreachable!("modo comprobado arriba"),
            };
            self.mode = Mode::Released(Box::new(layer));
            self.status.release = Some(ReleaseReason::OxygenAboveEngage);
            events.push(CarryEvent::Released(ReleaseReason::OxygenAboveEngage));
            return Ok(());
        }
        // Avance en temporales.
        let elapsed = lock.elapsed + dt;
        let q = self.plan.progress(elapsed);
        let tip = lock.tip_at(q);
        let (vertices, normals, step, distal) = if tip == lock.t0 {
            (
                lock.held.vertices().to_vec(),
                lock.held.normals().to_vec(),
                0.0,
                lock.held.pose(),
            )
        } else {
            let (v, n, pose, step) = lock.held.evaluate([tip.x, tip.y, tip.z], lock.memory)?;
            (v, n, step, pose)
        };
        let bounds = sphere(&vertices).ok_or("esfera del tramo no finita; nada cambia")?;
        let (position, orientation) = if tip == lock.t0 && same_bits(&body, &lock.body0) {
            lock.ship0
        } else {
            let s = body * distal_frame(&distal) * lock.rel;
            if !s.m.iter().flatten().all(|x| x.is_finite()) {
                return Err("pose de nave del arrastre no finita; nada cambia".into());
            }
            ship_pose_from_render(&s)
        };
        let (drag_started, held_reached) = (
            lock.drag_started || elapsed > self.plan.hold_s,
            lock.held_reached || q >= 1.0,
        );
        // Publicacion: capa (con respaldo), validacion de la capa en el World y commit de la nave.
        let Mode::Locked(lock) = &mut self.mode else {
            return Err("sin bloqueo".into());
        };
        let (nv, nn) = lock.layer.prefix;
        if lock.layer.model.vertices.len() != nv + vertices.len()
            || lock.layer.model.normals.len() != nn + normals.len()
        {
            return Err("tramo con otra topologia que la capa; nada cambia".into());
        }
        let backup = (
            lock.layer.model.vertices[nv..].to_vec(),
            lock.layer.model.normals[nn..].to_vec(),
            lock.layer.bounds,
        );
        lock.layer.model.vertices[nv..].copy_from_slice(&vertices);
        lock.layer.model.normals[nn..].copy_from_slice(&normals);
        lock.layer.bounds = bounds;
        let published = world
            .validate_solar_body_override(&lock.layer.view())
            .and_then(|_| world.commit_captured_ship_pose(position, orientation));
        if let Err(e) = published {
            lock.layer.model.vertices[nv..].copy_from_slice(&backup.0);
            lock.layer.model.normals[nn..].copy_from_slice(&backup.1);
            lock.layer.bounds = backup.2;
            return Err(e);
        }
        if drag_started && !lock.drag_started {
            events.push(CarryEvent::DragStarted);
        }
        if held_reached && !lock.held_reached {
            events.push(CarryEvent::HeldReached);
        }
        (lock.elapsed, lock.progress, lock.tip) = (elapsed, q, tip);
        (lock.drag_started, lock.held_reached) = (drag_started, held_reached);
        self.status.elapsed_s = elapsed;
        self.status.progress = q;
        self.status.tip_radius_obj = Some((tip - lock.center).length());
        self.status.last_step_obj = step;
        Ok(())
    }

    /// Escape EXPLICITO ante escena perdida o cuadro imposible: suelta la nave (misma pose, sin inercia),
    /// descarta la capa propia (sin render viejo) y deja la preview en `Aborted`. Infalible; X reinicia.
    pub fn abort(&mut self, world: &mut World, reason: &str) {
        if matches!(self.mode, Mode::Locked(_)) {
            free_ship(world);
        }
        self.mode = Mode::Aborted;
        self.status.error = Some(reason.to_string());
        self.status.phase = CarryPhase::Aborted;
        self.status.events = vec![CarryEvent::Aborted(reason.to_string())];
    }

    fn phase(&self) -> CarryPhase {
        match &self.mode {
            Mode::Free => CarryPhase::Free(self.preview.status().phase),
            Mode::Locked(l) if l.held_reached => CarryPhase::Held,
            Mode::Locked(l) if l.drag_started => CarryPhase::Dragging,
            Mode::Locked(_) => CarryPhase::Captured,
            Mode::Released(_) => CarryPhase::Released,
            Mode::Aborted => CarryPhase::Aborted,
        }
    }
    /// Mientras hay bloqueo la entrada de vuelo efectiva es neutra y las vistas que recolocan, ignoradas.
    pub fn locks_flight(&self) -> bool {
        matches!(self.mode, Mode::Locked(_))
    }
    /// Capa propia (bloqueo u observacion) o la de C sin bloqueo; abortada: ninguna.
    fn has_own_layer(&self) -> bool {
        matches!(self.mode, Mode::Locked(_) | Mode::Released(_))
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn plan(&self) -> &CarryPlan {
        &self.plan
    }
    pub fn status(&self) -> CarryStatus {
        self.status.clone()
    }
    pub fn preview(&self) -> &GraspPreview<'c, 'p> {
        &self.preview
    }
    fn own_layer(&self) -> Option<&Layer> {
        match &self.mode {
            Mode::Locked(l) => Some(&l.layer),
            Mode::Released(l) => Some(l),
            _ => None,
        }
    }
    /// Capa `Body` dibujada.
    pub fn body_override(&self) -> Option<SolarBodyOverride<'_>> {
        match &self.mode {
            Mode::Free => self.preview.body_override(),
            Mode::Aborted => None,
            _ => self.own_layer().map(Layer::view),
        }
    }
    /// Tramo dibujado (vertices, normales) en OBJ de la criatura.
    pub fn arm(&self) -> Option<ArmBuffers<'_>> {
        match &self.mode {
            Mode::Free => self.preview.arm(),
            Mode::Aborted => None,
            _ => self.own_layer().map(Layer::arm),
        }
    }
    /// Distancia (OBJ) del centroide grip de la MALLA dibujada al ancla aprobada transportada con la nave
    /// de render y el cuerpo ACTUALES del World (en C sin bloqueo: la medida de C).
    pub fn final_mesh_contact_error(&self, world: &World) -> Option<f64> {
        if !self.has_own_layer() {
            return match self.mode {
                Mode::Free => self.preview.final_mesh_contact_error(),
                _ => None,
            };
        }
        let (ship, body) = study_transforms(world).ok()?;
        let adapter = self.preview.clip().adapter();
        let desired = adapter.anchor().desired_obj(&ship, &body).ok()?;
        let (v, _) = self.arm()?;
        let mut sum = Vec3::ZERO;
        for &i in adapter.grip() {
            sum += Vec3::from_array(*v.get(i)?);
        }
        let d = (sum * (1.0 / adapter.grip().len() as f64) - Vec3::from_array(desired)).length();
        d.is_finite().then_some(d)
    }
    /// Centroide grip de la malla dibujada y ancla transportada, en MUNDO (marcas de diagnostico).
    pub fn contact_points_world(&self, world: &World) -> Option<(Vec3, Vec3)> {
        if !self.has_own_layer() {
            return match self.mode {
                Mode::Free => self.preview.contact_points_world(world),
                _ => None,
            };
        }
        let (ship, body) = study_transforms(world).ok()?;
        let adapter = self.preview.clip().adapter();
        let desired = adapter.anchor().desired_obj(&ship, &body).ok()?;
        let (v, _) = self.arm()?;
        let mut sum = Vec3::ZERO;
        for &i in adapter.grip() {
            sum += Vec3::from_array(*v.get(i)?);
        }
        let c = sum * (1.0 / adapter.grip().len() as f64);
        Some((
            body.transform_point(c),
            body.transform_point(Vec3::from_array(desired)),
        ))
    }
    /// Estado para el titulo de la ventana (sin HUD).
    pub fn title(&self) -> String {
        let s = &self.status;
        match s.phase {
            CarryPhase::Free(_) => self.preview.title(),
            CarryPhase::Captured => format!(
                "NAVE CAPTURADA: vuelo bloqueado (agarre {:.2}/{:.2} s) | {RESET_KEY} suelta",
                s.elapsed_s, self.plan.hold_s
            ),
            CarryPhase::Dragging => format!(
                "ARRASTRE hacia la criatura {:.0} % | vuelo bloqueado | {RESET_KEY} suelta",
                s.progress * 100.0
            ),
            CarryPhase::Held => format!(
                "RETENIDA junto a la criatura (punta a {:.3} OBJ del lazo) | vuelo bloqueado | {RESET_KEY} suelta",
                s.tip_radius_obj.unwrap_or(f64::NAN)
            ),
            CarryPhase::Released => format!(
                "SUELTA ({}): ultima malla como OBSERVACION, NO es sujecion | nave libre | {RESET_KEY} reinicia",
                match s.release {
                    Some(ReleaseReason::OxygenAboveEngage) => "O2 > 15 %",
                    _ => "reinicio rechazado",
                }
            ),
            CarryPhase::Aborted => format!(
                "ABORTADA: {} | nave libre, sin tentaculo | {RESET_KEY} reinicia",
                s.error.as_deref().unwrap_or("?")
            ),
        }
    }
}

/// Controles ya leidos de un cuadro (ventana y headless).
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameControls {
    pub ship: ShipInput,
    pub oxygen: OxygenKeys,
    pub solar: SolarKeys,
    pub preview: PreviewKeys,
}

#[derive(Clone, Debug)]
pub struct FrameOutcome {
    /// Entrada que recibio el World (neutra con bloqueo).
    pub effective_ship: ShipInput,
    /// Teclas solares aplicadas (sin R/7/8 con bloqueo).
    pub solar_applied: SolarKeys,
    /// Se pidio una vista que recoloca la nave durante el bloqueo y se ignoro.
    pub solar_blocked: bool,
    pub status: Result<CarryStatus, String>,
    /// El cuadro fallo con capa propia y se ejecuto el escape `abort`.
    pub aborted: bool,
}

/// Cuadro COMPARTIDO por ventana, headless y pruebas, en el orden del lanzador: teclas solares (sin
/// recolocar la nave si hay bloqueo) -> World con la entrada EFECTIVA y los segundos reales (O2 incluido)
/// -> preview/arrastre con `dt = min(elapsed, MAX_PHYSICS_DT)` (commit de la nave y camara) -> render (lo
/// hace el llamador). Si el cuadro falla con capa propia, `abort` explicito (nave libre, sin capa).
pub fn advance_capture_frame(
    world: &mut World,
    game: &mut SolarGame,
    oxygen: &mut OxygenTransition,
    carry: &mut CaptureCarry,
    controls: FrameControls,
    elapsed: f64,
) -> Result<FrameOutcome, String> {
    // Dominio numerico ANTES de H/G/R o cualquier otra mutacion. Reutilizar los guards
    // de O2 sobre una copia conserva exactamente sus prioridades y semantica.
    let mut oxygen_candidate = *oxygen;
    oxygen_candidate.press(controls.oxygen)?;
    oxygen_candidate.advance(elapsed)?;
    let raw = controls.ship;
    if ![raw.thrust, raw.yaw, raw.pitch, raw.roll]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err("entrada numerica de vuelo no finita; nada cambia".into());
    }
    // Ship es Copy: ejercitar la fisica existente sin escribir World (tambien si el
    // bloqueo enmascararia la entrada). No limitar controles finitos ni cambiar ajustes.
    let mut ship_candidate = world.ship;
    ship_candidate.update(&raw, elapsed.min(MAX_PHYSICS_DT));
    let render = ship_candidate.transform(SHIP_MODEL_YAW).matrix();
    let composed = world.camera.view_projection() * render;
    let axes = [0, 1, 2].map(|c| ship_candidate.orientation.column(c));
    if !(ship_candidate.position.is_finite()
        && ship_candidate.speed.is_finite()
        && axes
            .iter()
            .all(|a| a.is_finite() && (a.length() - 1.0).abs() <= 1e-9)
        && render.m.iter().flatten().all(|v| v.is_finite())
        && composed.m.iter().flatten().all(|v| v.is_finite())
        && world.ship_model.vertices.iter().all(|&v| {
            let clip = composed.transform(Vec3::from_array(v).extend(1.0));
            [clip.x, clip.y, clip.z, clip.w]
                .iter()
                .all(|v| v.is_finite())
        }))
    {
        return Err("composicion numerica del vuelo no finita o degenerada; nada cambia".into());
    }
    // CORRECTION05: identidad EXACTA (inmutable) ANTES de teclas solares, tick, O2 o captura: el tick de este
    // cuadro puede alcanzar el terminal y su retorno congelado no debe aceptar una criatura PRESENTE ajena.
    // Sin criatura (escena perdida) se conserva el escape aprobado: tick y `abort` explicito mas abajo.
    if world.creature().is_some() {
        carry.validate_frozen(world, elapsed.min(MAX_PHYSICS_DT))?;
    }
    if world.gameplay.game_over || world.gameplay.objective_complete {
        // CORRECTION03 (USER_DECISIONS): sin O2 real o con victoria, nave y captura quedan congeladas
        // hasta el reinicio completo; solo un World ENLAZADO (identidad exacta) se congela, sin mutar nada.
        carry.validate_frozen(world, elapsed.min(MAX_PHYSICS_DT))?;
        return Ok(FrameOutcome {
            effective_ship: ShipInput::default(),
            solar_applied: SolarKeys::default(),
            solar_blocked: controls.solar.reset || controls.solar.frontal || controls.solar.oblique,
            status: Ok(carry.status()),
            aborted: false,
        });
    }
    let locked = carry.locks_flight();
    let mut solar = controls.solar;
    let solar_blocked = locked && (solar.reset || solar.frontal || solar.oblique);
    if locked {
        (solar.reset, solar.frontal, solar.oblique) = (false, false, false);
    }
    let effective_ship = if locked {
        ShipInput::default()
    } else {
        controls.ship
    };
    game.apply(world, solar)?;
    advance_perception_frame(world, oxygen, controls.oxygen, &effective_ship, elapsed)?;
    if world.gameplay.game_over || world.gameplay.objective_complete {
        // CORRECTION04: el tick REAL de este cuadro alcanzo el terminal (O2 agotado o victoria). Como en la ruta
        // automatica, el arrastre ya no avanza reloj/pose/malla ni acepta X/T hasta el reinicio completo.
        return Ok(FrameOutcome {
            effective_ship,
            solar_applied: solar,
            solar_blocked,
            status: Ok(carry.status()),
            aborted: false,
        });
    }
    let status = carry.frame(world, elapsed.min(MAX_PHYSICS_DT), controls.preview);
    let aborted = status.is_err() && carry.has_own_layer();
    if let (true, Err(e)) = (aborted, &status) {
        carry.abort(world, e);
    }
    Ok(FrameOutcome {
        effective_ship,
        solar_applied: solar,
        solar_blocked,
        status,
        aborted,
    })
}

/// Ayuda minima (lanzador y ventana).
pub fn help_text() -> &'static str {
    "CAPTURA Y ARRASTRE EXPERIMENTAL (preview aislada, NO aprobada): persecucion -> traspaso B1 -> preparacion y cierre P1 (aprobados) -> CAPTURA -> ARRASTRE -> RETENIDA.
Arranca al 100 % de O2. Pulsa 4 (objetivo 5 %): la punta persigue la nave. Pulsa T para pedir el agarre (puede ser antes de que llegue).
Empieza cuando la punta llego y la nave esta QUIETA. Hasta el agarre todo es como la preview C aprobada (mover la nave o subir el O2 cancela).
Al agarrar, la nave queda CAPTURADA: el vuelo (W/S, giros, alabeo, Shift) y las vistas que recolocan la nave (R, 7, 8) se ignoran.
Tras 0,5 s el tentaculo RETRAE su tramo y ARRASTRA la nave hacia la criatura durante 3,5 s; despues la RETIENE junto a ella.
X suelta en cualquier momento: la nave queda donde esta, sin inercia, y vuelve a volar; la punta reinicia la persecucion.
Subir el O2 por encima del 15 % (1-3, M) tambien suelta: queda la ultima malla como OBSERVACION y la nave libre; X reinicia.
H y G siguen activas (H mueve el cuerpo y la nave sujeta lo acompana). Esc sale. P guarda una captura (con --capture-dir)."
}

// ---- GT3-AUTO (aditivo; quitar este bloque restaura el aprobado173 byte a byte) ----

impl CaptureCarry<'_, '_> {
    /// Un cuadro de la politica AUTOMATICA (GT3): igual que `frame` salvo que, sin bloqueo, la preview avanza
    /// con `GraspPreview::frame_on_contact` (captura en el contacto real del cuadro, sin `T` ni quietud). El
    /// bloqueo en el Gripped vigente, el arrastre H1, la retencion, la suelta por O2, el `Err` atomico y el
    /// escape `abort` son los aprobados (mismas funciones). `reset` es la X de diagnostico de siempre.
    pub fn frame_on_contact(
        &mut self,
        world: &mut World,
        dt: f64,
        reset: bool,
    ) -> Result<CarryStatus, String> {
        self.frame_on_contact_owned(world, dt, reset, false)
    }

    pub(crate) fn frame_on_contact_owned(
        &mut self,
        world: &mut World,
        dt: f64,
        reset: bool,
        defer_oxygen: bool,
    ) -> Result<CarryStatus, String> {
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}; nada cambia"
            ));
        }
        if let Some(layer) = self.own_layer() {
            let model = world
                .creature()
                .ok_or("el World no tiene criatura")?
                .model();
            let body = world
                .staged_creature_parts()
                .ok_or("el World no tiene particion")?
                .body();
            if !same_model(&layer.source_model, model) || !same_model(&layer.source_body, body) {
                return Err("criatura o particion distintas del bloqueo; nada cambia".into());
            }
            world.validate_solar_body_override(&layer.view())?;
        }
        let keys = PreviewKeys {
            request: false,
            reset,
        };
        let mut events = Vec::new();
        match &self.mode {
            Mode::Free => {
                let st = self
                    .preview
                    .frame_on_contact_owned(world, dt, reset, defer_oxygen)?;
                events.extend(st.events.iter().cloned().map(CarryEvent::Preview));
                if st.events.contains(&PreviewEvent::Reset) {
                    self.clear_lock_status();
                }
                let gripped_now =
                    st.events.contains(&PreviewEvent::Gripped) && st.phase == PreviewPhase::Gripped;
                self.status.preview = st;
                if self.enabled && gripped_now {
                    match self.acquire(world) {
                        Ok(lock) => {
                            self.status.elapsed_s = 0.0;
                            self.status.progress = 0.0;
                            self.status.last_step_obj = 0.0;
                            self.status.tip_radius_obj = Some(lock.r0);
                            self.status.r0_obj = Some(lock.r0);
                            self.status.r_end_obj = Some(lock.r_end.min(lock.r0));
                            self.status.release = None;
                            self.status.error = None;
                            self.mode = Mode::Locked(lock);
                            events.push(CarryEvent::Locked);
                        }
                        Err(e) => events.push(CarryEvent::CarryRejected(e)),
                    }
                }
            }
            Mode::Locked(_) => self.locked_frame(world, dt, keys, &mut events, defer_oxygen)?,
            Mode::Released(_) | Mode::Aborted => {
                if let Mode::Released(layer) = &self.mode {
                    world.validate_solar_body_override(&layer.view())?;
                }
                if reset {
                    self.reset_preview(world, dt, &mut events)?;
                }
            }
        }
        self.status.events = events;
        self.status.phase = self.phase();
        self.status.frame += 1;
        Ok(self.status.clone())
    }
    /// La capa dibujada es propia del bloqueo o de su observacion (no la de la preview).
    pub fn has_carry_layer(&self) -> bool {
        self.has_own_layer()
    }
}
