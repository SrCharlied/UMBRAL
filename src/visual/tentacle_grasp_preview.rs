//! GT1 fase C: preview manual AISLADA de la cadena real persecucion espacial -> llegada vigente -> B1 ->
//! P1 (preparacion local + cierre + hold). Ver `docs/tentacle-grasp-preview.md` y `PREVIEW_CONTRACT.json`.
//! No lo llaman `app`/`main`; nunca escribe World, nave, camara, input ni O2 (recibe `&World` despues de la
//! fisica de cada cuadro). Las matrices fijas de B1/P1 se traducen en CANCELACION (observacion), nunca en
//! bloquear la nave. Sin ajustes del nucleo, del arte ni de la camara.
// Bucles por indice sobre componentes xyz: estilo, no logica.
#![allow(clippy::needless_range_loop)]

use crate::{
    assets::{
        obj::{Face, Model, parse_obj},
        ship_model_path,
    },
    math::{Mat4, Vec3},
    scene::world::{SHIP_MODEL_YAW, SolarBodyOverride, World},
    visual::{
        tentacle_grasp_closure::{ClosureClip, ClosurePhase, ClosureStatus, GraspClosure},
        tentacle_grasp_handoff::{
            GraspHandoff, HANDOFF_CANDIDATE_DURATION_S, HandoffPhase, HandoffStatus, ReachedPose,
        },
        tentacle_growth_cpu::{
            GrowthPackage,
            capture::{CaptureManifest, PackageFile, fnv1a64},
        },
        tentacle_live_pursuit::{MAX_PURSUIT_DT, PURSUIT_ENGAGE_OXYGEN, PursuitMode, same_model},
        tentacle_solar_game::{SolarCaptureDebug, TentacleCaptureRequest},
        tentacle_spatial_pursuit::{SpatialFrameStatus, SpatialPursuit, SpatialRig},
    },
};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

/// Tecla libre comprobada para pedir/armar el agarre (G y R ya son de la ruta solar).
pub const REQUEST_KEY: &str = "T";
/// Tecla libre comprobada para reiniciar el experimento.
pub const RESET_KEY: &str = "X";
/// Cuadros SEGUIDOS con matrices de nave dibujada y cuerpo iguales (bits) para considerar la nave quieta:
/// con `dt` variable de ventana un solo cuadro igual puede ser casual (cancelaria al siguiente).
pub const QUIET_FRAMES: u32 = 10;

/// Entradas de la preview leidas UNA vez: la persecucion (duena de su paquete) y un `GrowthPackage`
/// inmutable construido con los MISMOS bytes para prestarlo a `ClosureClip`.
pub struct PreviewInputs {
    pub debug: SolarCaptureDebug,
    pub package: GrowthPackage,
}

fn copy_model(m: &Model) -> Model {
    Model {
        vertices: m.vertices.clone(),
        uvs: m.uvs.clone(),
        normals: m.normals.clone(),
        faces: m
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: m.material_libraries.clone(),
        materials: m.materials.clone(),
        bounds_center: m.bounds_center,
        bounds_radius: m.bounds_radius,
    }
}

/// Lee la captura (manifest y cada OBJ verificado por tamano y FNV-1a sobre los mismos bytes), el paquete
/// de crecimiento y la correspondencia UNA vez, y construye con esos bytes `SolarCaptureDebug::from_parts`
/// y `GrowthPackage::from_parts` (mismas validaciones que `SolarCaptureDebug::load`).
pub fn load_preview_inputs(request: &TentacleCaptureRequest) -> Result<PreviewInputs, String> {
    request.validate()?;
    let dir = &request.capture_dir;
    let text = std::fs::read_to_string(dir.join("capture_package.txt"))
        .map_err(|e| format!("capture_package.txt: {e}"))?;
    let manifest = CaptureManifest::parse(&text)?;
    let read = |f: &PackageFile| -> Result<Model, String> {
        let bytes =
            std::fs::read(dir.join(&f.relative)).map_err(|e| format!("{}: {e}", f.relative))?;
        if bytes.len() as u64 != f.bytes || fnv1a64(&bytes) != f.fnv1a64 {
            return Err(format!(
                "{}: tamano o huella distintos del manifest (paquete ajeno)",
                f.relative
            ));
        }
        let source =
            String::from_utf8(bytes).map_err(|_| format!("{}: no es UTF-8", f.relative))?;
        parse_obj(&source).map_err(|e| format!("{}: {e}", f.relative))
    };
    let files: Vec<&PackageFile> = manifest.files().collect();
    let (body_file, arm_files) = files.split_first().ok_or("manifest sin archivos")?;
    let body = read(body_file)?;
    let arms = arm_files
        .iter()
        .map(|f| read(f))
        .collect::<Result<Vec<_>, _>>()?;
    let read_text =
        |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let growth = read_text(&request.growth_package)?;
    let correspondence = read_text(&request.correspondence)?;
    let package = GrowthPackage::from_parts(
        &text,
        arms.iter().map(copy_model).collect(),
        copy_model(&body),
        &growth,
    )?;
    let debug = SolarCaptureDebug::from_parts(&text, arms, body, &growth, &correspondence)?;
    Ok(PreviewInputs { debug, package })
}

/// Reposo del brazo en el Meshy del World (vertices de la correspondencia de la persecucion enlazada).
pub fn arm_rest(world: &World, pursuit: &SpatialPursuit) -> Result<Vec<[f64; 3]>, String> {
    let model = world
        .creature()
        .ok_or("el World no tiene criatura")?
        .model();
    pursuit
        .debug()
        .correspondence()
        .arm_vertex()
        .iter()
        .map(|&i| {
            model
                .vertices
                .get(i)
                .copied()
                .ok_or_else(|| "correspondencia fuera del Meshy".to_string())
        })
        .collect()
}

/// Huella de una entrada protegida.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputFingerprint {
    pub path: PathBuf,
    pub bytes: u64,
    pub fnv1a64: u64,
}

/// Entradas efectivas ABIERTAS durante la ejecucion (en Windows con FILE_SHARE_READ: rechaza escritores y
/// borrados): manifest, OBJ del paquete, growth, correspondencia, OBJ+MTL de nave y criatura y textura.
pub struct InputPins {
    inputs: Vec<(PathBuf, File)>,
}

impl InputPins {
    pub fn pin(
        request: &TentacleCaptureRequest,
        creature_model: &Path,
        creature_texture: &Path,
    ) -> Result<Self, String> {
        request.validate()?;
        let manifest_path = request.capture_dir.join("capture_package.txt");
        let mut pins = Self { inputs: Vec::new() };
        for p in [
            &manifest_path,
            &request.growth_package,
            &request.correspondence,
            creature_texture,
        ] {
            pins.add(p)?;
        }
        pins.add_model(&ship_model_path())?;
        pins.add_model(creature_model)?;
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("capture_package.txt: {e}"))?;
        for f in CaptureManifest::parse(&text)?.files() {
            pins.add(&request.capture_dir.join(&f.relative))?;
        }
        Ok(pins)
    }
    fn add_model(&mut self, path: &Path) -> Result<(), String> {
        self.add(path)?;
        let source = std::fs::read_to_string(path)
            .map_err(|e| format!("proteccion de modelo {}: {e}", path.display()))?;
        for lib in parse_obj(&source)?.material_libraries {
            let rel = Path::new(&lib);
            if rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("El MTL debe estar dentro de la carpeta del modelo".into());
            }
            self.add(&path.parent().unwrap_or_else(|| Path::new(".")).join(rel))?;
        }
        Ok(())
    }
    fn add(&mut self, path: &Path) -> Result<(), String> {
        let mut open = OpenOptions::new();
        open.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            open.share_mode(1);
        }
        let file = open
            .open(path)
            .map_err(|e| format!("proteccion de entrada {}: {e}", path.display()))?;
        self.inputs.push((path.to_path_buf(), file));
        Ok(())
    }
    pub fn paths(&self) -> Vec<&Path> {
        self.inputs.iter().map(|(p, _)| p.as_path()).collect()
    }
    /// Tamano y FNV-1a de cada entrada (lectura compartida; no escribe).
    pub fn fingerprints(&self) -> Result<Vec<InputFingerprint>, String> {
        self.inputs
            .iter()
            .map(|(p, _)| {
                let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
                Ok(InputFingerprint {
                    path: p.clone(),
                    bytes: bytes.len() as u64,
                    fnv1a64: fnv1a64(&bytes),
                })
            })
            .collect()
    }
}

/// Tramo publicado: (vertices, normales) en OBJ de la criatura.
pub type ArmBuffers<'a> = (&'a [[f64; 3]], &'a [[f64; 3]]);

/// Teclas del cuadro para la preview (ya leidas).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreviewKeys {
    /// `T`: armar/desarmar la solicitud de agarre.
    pub request: bool,
    /// `X`: reiniciar el experimento.
    pub reset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewPhase {
    /// Ruta espacial anterior (con o sin solicitud armada).
    Pursuit,
    /// Traspaso B1 desde la llegada capturada.
    Handoff,
    /// P1: preparacion local autorada (referencia .. cierre).
    Preparing,
    /// P1: cierre autorado (cierre .. grip_frame).
    Closing,
    /// P1: grip_frame publicado; hold.
    Gripped,
    /// Observacion CANCELADA: ultima malla congelada, no es contacto vigente.
    Cancelled,
    /// Observacion con ERROR del nucleo: ultima malla congelada.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    /// Matrices de nave dibujada o cuerpo distintas (bits) de las capturadas.
    ShipOrBodyMoved,
    /// O2 mostrado por encima del umbral de persecucion.
    OxygenAboveEngage,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreviewEvent {
    Armed,
    Disarmed,
    RequestIgnored,
    Captured,
    ClosureStarted,
    Gripped,
    Cancelled(CancelReason),
    Failed(String),
    RequestRejected(String),
    PursuitStepRejected(String),
    Reset,
    ResetRejected(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewStatus {
    pub phase: PreviewPhase,
    pub armed: bool,
    /// Cuadros procesados por `frame` con exito.
    pub frame: u64,
    /// Eventos del ultimo cuadro.
    pub events: Vec<PreviewEvent>,
    /// Matrices de nave dibujada y cuerpo iguales (bits) a las del cuadro anterior.
    pub matrices_stable: bool,
    /// Cuadros seguidos con `matrices_stable` (la solicitud exige `QUIET_FRAMES`).
    pub quiet_frames: u32,
    /// Ultimo paso PUBLICADO de la persecucion (en el episodio: el de la llegada capturada).
    pub pursuit: Option<SpatialFrameStatus>,
    pub handoff: Option<HandoffStatus>,
    pub closure: Option<ClosureStatus>,
    /// `frame` del cuadro de la captura del episodio vigente.
    pub captured_at_frame: Option<u64>,
    pub cancel: Option<CancelReason>,
    pub error: Option<String>,
    /// Fase activa al cancelar o fallar.
    pub stopped_in: Option<PreviewPhase>,
}

/// Igualdad BIT A BIT (NaN nunca coincide con nada, -0 != +0).
fn same_bits(a: &Mat4, b: &Mat4) -> bool {
    a.m.iter()
        .flatten()
        .zip(b.m.iter().flatten())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}

/// Matrices de nave DIBUJADA y cuerpo del World actual (las mismas que capturan B1/P1).
pub fn study_transforms(world: &World) -> Result<(Mat4, Mat4), String> {
    let body = world
        .creature_render_transform()
        .ok_or("el World no tiene criatura")?
        .matrix();
    Ok((world.ship.transform(SHIP_MODEL_YAW).matrix(), body))
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

type V3 = [f64; 3];

/// Captura de un episodio: matrices, pose alcanzada y capa propia (COPIA de la capa compuesta).
struct Episode {
    /// Immutable complete source identities from the successful current pursuit step, not the
    /// deformed layer, sizes, bounds or one pose. Body includes its exact face partition.
    source_model: Model,
    source_body: Model,
    ship: Mat4,
    body: Mat4,
    reached: ReachedPose,
    layer: Model,
    prefix: (usize, usize),
    bounds: (Vec3, f64),
}

impl Episode {
    /// Escribe el tramo SOLO si es finito y de la topologia de la capa; si no, `Err` y nada cambia.
    fn publish(&mut self, vertices: &[V3], normals: &[V3]) -> Result<(), String> {
        let (nv, nn) = self.prefix;
        if self.layer.vertices.len() != nv + vertices.len()
            || self.layer.normals.len() != nn + normals.len()
        {
            return Err("tramo con otra topologia que la capa capturada".into());
        }
        if normals.iter().any(|p| !p.iter().all(|x| x.is_finite())) {
            return Err("normales del tramo no finitas".into());
        }
        let bounds = sphere(vertices).ok_or("esfera del tramo no finita")?;
        self.layer.vertices[nv..].copy_from_slice(vertices);
        self.layer.normals[nn..].copy_from_slice(normals);
        self.bounds = bounds;
        Ok(())
    }
    fn arm(&self) -> (&[V3], &[V3]) {
        let (nv, nn) = self.prefix;
        (&self.layer.vertices[nv..], &self.layer.normals[nn..])
    }
}

enum Stage<'c, 'p> {
    Pursuit,
    Handoff(Box<GraspHandoff<'c>>),
    Closure(Box<GraspClosure<'c, 'p>>),
    /// Observacion cancelada o con error (la capa del episodio queda tal cual).
    Stopped,
}

/// Estado de la preview: posee la persecucion; B1 y P1 toman prestado el clip (externo) durante `'c`.
/// Nunca escribe el World: recibe `&World` despues de la fisica de cada cuadro.
pub struct GraspPreview<'c, 'p> {
    clip: &'c ClosureClip<'p>,
    pursuit: SpatialPursuit,
    stage: Stage<'c, 'p>,
    episode: Option<Episode>,
    previous: Option<(Mat4, Mat4)>,
    status: PreviewStatus,
}

/// `Err` si el rig enlazado de la persecucion no es, bit a bit, el del clip (otro paquete o reposo).
fn check_same_rig(clip: &ClosureClip, pursuit: &SpatialPursuit) -> Result<(), String> {
    let rig = pursuit
        .rig()
        .ok_or("persecucion sin enlazar (bind) para la preview")?;
    let own = clip.adapter().rig();
    if rig.rest_vertices() != own.rest_vertices() || rig.rest_normals() != own.rest_normals() {
        return Err(
            "el rig enlazado de la persecucion no es el del clip P1 (paquete o reposo ajeno); rechazado"
                .into(),
        );
    }
    Ok(())
}

impl<'c, 'p> GraspPreview<'c, 'p> {
    /// Envuelve una persecucion YA enlazada (`bind`) cuyo rig coincide bit a bit con el del clip.
    pub fn new(clip: &'c ClosureClip<'p>, pursuit: SpatialPursuit) -> Result<Self, String> {
        check_same_rig(clip, &pursuit)?;
        Ok(Self {
            clip,
            status: PreviewStatus {
                phase: PreviewPhase::Pursuit,
                armed: false,
                frame: 0,
                events: Vec::new(),
                matrices_stable: false,
                quiet_frames: 0,
                pursuit: pursuit.last(),
                handoff: None,
                closure: None,
                captured_at_frame: None,
                cancel: None,
                error: None,
                stopped_in: None,
            },
            pursuit,
            stage: Stage::Pursuit,
            episode: None,
            previous: None,
        })
    }

    /// Un cuadro DESPUES de la fisica del World (`dt` = el de la persecucion). Con `dt` invalido o sin
    /// criatura devuelve `Err` y NADA cambia. Los demas fallos no abortan la preview: quedan como eventos,
    /// solicitud rechazada u observacion con error, sin capa parcial.
    pub fn frame(
        &mut self,
        world: &World,
        dt: f64,
        keys: PreviewKeys,
    ) -> Result<PreviewStatus, String> {
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}; nada cambia"
            ));
        }
        let current = study_transforms(world)?;
        // Episode paths do not call SpatialPursuit::step. Revalidate their immutable binding and
        // explicit Body render contract BEFORE reset, stage extraction, clocks or publication.
        if let Some(e) = &self.episode {
            let model = world
                .creature()
                .ok_or("el World no tiene criatura")?
                .model();
            let parts = world
                .staged_creature_parts()
                .ok_or("el World no tiene particion")?;
            if !same_model(&e.source_model, model) || !same_model(&e.source_body, parts.body()) {
                return Err(
                    "criatura o particion distintas del episodio capturado; nada cambia".into(),
                );
            }
            world.validate_solar_body_override(&SolarBodyOverride {
                model: &e.layer,
                extra_bounds_obj: e.bounds,
            })?;
        }
        let stable = self
            .previous
            .is_some_and(|(s, b)| same_bits(&s, &current.0) && same_bits(&b, &current.1));
        let quiet = if stable {
            self.status.quiet_frames.saturating_add(1)
        } else {
            0
        };
        let mut events = Vec::new();
        if keys.reset {
            match self.reset(world) {
                Ok(()) => events.push(PreviewEvent::Reset),
                Err(e) => events.push(PreviewEvent::ResetRejected(e)),
            }
        }
        match std::mem::replace(&mut self.stage, Stage::Stopped) {
            Stage::Pursuit => {
                self.stage = Stage::Pursuit;
                self.pursuit_frame(world, dt, keys, current, quiet >= QUIET_FRAMES, &mut events);
            }
            Stage::Handoff(handoff) => {
                if keys.request {
                    events.push(PreviewEvent::RequestIgnored);
                }
                self.handoff_frame(world, dt, handoff, current, &mut events);
            }
            Stage::Closure(closure) => {
                if keys.request {
                    events.push(PreviewEvent::RequestIgnored);
                }
                self.closure_frame(world, dt, closure, current, &mut events);
            }
            Stage::Stopped => {
                if keys.request {
                    events.push(PreviewEvent::RequestIgnored);
                }
            }
        }
        self.previous = Some(current);
        self.status.phase = self.phase();
        self.status.events = events;
        self.status.matrices_stable = stable;
        self.status.quiet_frames = quiet;
        self.status.frame += 1;
        Ok(self.status.clone())
    }

    /// X: reenlaza la persecucion con el World ACTUAL (controlador a reposo) y descarta episodio y
    /// solicitud. Comprueba ANTES que el reposo actual da el rig del clip; con `Err` nada cambia.
    fn reset(&mut self, world: &World) -> Result<(), String> {
        let rest = arm_rest(world, &self.pursuit)?;
        let probe = SpatialRig::new(self.clip.package().data(), &rest)?;
        let own = self.clip.adapter().rig();
        if probe.rest_vertices() != own.rest_vertices()
            || probe.rest_normals() != own.rest_normals()
        {
            return Err(
                "el World actual no tiene el reposo del clip P1; reinicio rechazado".into(),
            );
        }
        self.pursuit.bind(world)?;
        self.stage = Stage::Pursuit;
        self.episode = None;
        self.status = PreviewStatus {
            phase: PreviewPhase::Pursuit,
            armed: false,
            frame: self.status.frame,
            events: Vec::new(),
            matrices_stable: false,
            quiet_frames: self.status.quiet_frames,
            pursuit: None,
            handoff: None,
            closure: None,
            captured_at_frame: None,
            cancel: None,
            error: None,
            stopped_in: None,
        };
        if let Err(e) = check_same_rig(self.clip, &self.pursuit) {
            // Inconsistencia tras un bind valido: sin capa y sin episodio, visible en el titulo.
            self.stage = Stage::Stopped;
            self.status.error = Some(e.clone());
            self.status.stopped_in = Some(PreviewPhase::Pursuit);
            return Err(e);
        }
        Ok(())
    }

    fn pursuit_frame(
        &mut self,
        world: &World,
        dt: f64,
        keys: PreviewKeys,
        current: (Mat4, Mat4),
        quiet: bool,
        events: &mut Vec<PreviewEvent>,
    ) {
        if keys.request {
            self.status.armed = !self.status.armed;
            events.push(if self.status.armed {
                PreviewEvent::Armed
            } else {
                PreviewEvent::Disarmed
            });
        }
        let step = match self.pursuit.step(world, dt) {
            Ok(s) => s,
            Err(e) => {
                events.push(PreviewEvent::PursuitStepRejected(e));
                return;
            }
        };
        self.status.pursuit = Some(step);
        let arrived =
            step.step.reached && step.step.mode == PursuitMode::Pursuing && step.shaft.is_some();
        if !(self.status.armed && arrived && quiet) {
            return;
        }
        // Llegada ACTUAL (paso de este cuadro) y nave quieta: captura y B1 con las matrices actuales.
        let clip: &'c ClosureClip<'p> = self.clip;
        let started = (|| {
            check_same_rig(clip, &self.pursuit)?;
            let reached = ReachedPose::capture(&self.pursuit)?;
            let handoff = GraspHandoff::start(
                clip.adapter(),
                &reached,
                &current.0,
                &current.1,
                HANDOFF_CANDIDATE_DURATION_S,
            )?;
            let layer = copy_model(self.pursuit.composite().ok_or("persecucion sin capa")?);
            let prefix = self
                .pursuit
                .body_prefix()
                .ok_or("persecucion sin prefijo")?;
            let mut episode = Episode {
                source_model: copy_model(
                    world
                        .creature()
                        .ok_or("el World no tiene criatura")?
                        .model(),
                ),
                source_body: copy_model(
                    world
                        .staged_creature_parts()
                        .ok_or("el World no tiene particion")?
                        .body(),
                ),
                ship: current.0,
                body: current.1,
                reached,
                layer,
                prefix,
                bounds: (Vec3::ZERO, 0.0),
            };
            episode.publish(handoff.vertices(), handoff.normals())?;
            Ok::<_, String>((handoff, episode))
        })();
        self.status.armed = false;
        match started {
            Ok((handoff, episode)) => {
                self.status.handoff = Some(handoff.status());
                self.status.closure = None;
                self.status.captured_at_frame = Some(self.status.frame);
                self.episode = Some(episode);
                self.stage = Stage::Handoff(Box::new(handoff));
                events.push(PreviewEvent::Captured);
            }
            Err(e) => events.push(PreviewEvent::RequestRejected(e)),
        }
    }

    /// Politica de estudio ANTES de avanzar: matrices capturadas (bits) y O2 de persecucion.
    fn check_fresh(&self, world: &World, current: (Mat4, Mat4)) -> Option<CancelReason> {
        let e = self.episode.as_ref()?;
        if !(same_bits(&e.ship, &current.0) && same_bits(&e.body, &current.1)) {
            return Some(CancelReason::ShipOrBodyMoved);
        }
        (world.debug_oxygen01.is_nan() || world.debug_oxygen01 > PURSUIT_ENGAGE_OXYGEN)
            .then_some(CancelReason::OxygenAboveEngage)
    }

    fn stop(
        &mut self,
        during: PreviewPhase,
        cancel: Option<CancelReason>,
        error: Option<String>,
        events: &mut Vec<PreviewEvent>,
    ) {
        self.stage = Stage::Stopped;
        self.status.stopped_in = Some(during);
        match (cancel, error) {
            (Some(reason), _) => {
                self.status.cancel = Some(reason);
                events.push(PreviewEvent::Cancelled(reason));
            }
            (None, Some(e)) => {
                self.status.error = Some(e.clone());
                events.push(PreviewEvent::Failed(e));
            }
            (None, None) => {}
        }
    }

    fn handoff_frame(
        &mut self,
        world: &World,
        dt: f64,
        mut handoff: Box<GraspHandoff<'c>>,
        current: (Mat4, Mat4),
        events: &mut Vec<PreviewEvent>,
    ) {
        if let Some(reason) = self.check_fresh(world, current) {
            return self.stop(PreviewPhase::Handoff, Some(reason), None, events);
        }
        let (ship, body) = (current.0, current.1);
        let status = match handoff.advance(&ship, &body, dt) {
            Ok(s) => s,
            Err(e) => return self.stop(PreviewPhase::Handoff, None, Some(e), events),
        };
        let published = match self.episode.as_mut() {
            Some(ep) => ep.publish(handoff.vertices(), handoff.normals()),
            None => Err("episodio sin captura".into()),
        };
        if let Err(e) = published {
            return self.stop(PreviewPhase::Handoff, None, Some(e), events);
        }
        self.status.handoff = Some(status);
        if status.phase != HandoffPhase::Contact {
            self.stage = Stage::Handoff(handoff);
            return;
        }
        // Contact publicado: P1 arranca en este cuadro con una COPIA exacta (sin avanzar).
        match GraspClosure::start(self.clip, &handoff) {
            Ok(closure) => {
                self.status.closure = Some(closure.status());
                self.stage = Stage::Closure(Box::new(closure));
                events.push(PreviewEvent::ClosureStarted);
            }
            Err(e) => self.stop(PreviewPhase::Handoff, None, Some(e), events),
        }
    }

    fn closure_frame(
        &mut self,
        world: &World,
        dt: f64,
        mut closure: Box<GraspClosure<'c, 'p>>,
        current: (Mat4, Mat4),
        events: &mut Vec<PreviewEvent>,
    ) {
        let during = closure_phase(closure.status().phase);
        if let Some(reason) = self.check_fresh(world, current) {
            return self.stop(during, Some(reason), None, events);
        }
        let status = match closure.advance(&current.0, &current.1, dt) {
            Ok(s) => s,
            Err(e) => return self.stop(during, None, Some(e), events),
        };
        let published = match self.episode.as_mut() {
            Some(ep) => ep.publish(closure.vertices(), closure.normals()),
            None => Err("episodio sin captura".into()),
        };
        if let Err(e) = published {
            return self.stop(during, None, Some(e), events);
        }
        if status.phase == ClosurePhase::Gripped && during != PreviewPhase::Gripped {
            events.push(PreviewEvent::Gripped);
        }
        self.status.closure = Some(status);
        self.stage = Stage::Closure(closure);
    }

    fn phase(&self) -> PreviewPhase {
        match &self.stage {
            Stage::Pursuit => PreviewPhase::Pursuit,
            Stage::Handoff(_) => PreviewPhase::Handoff,
            Stage::Closure(c) => closure_phase(c.status().phase),
            Stage::Stopped if self.status.cancel.is_some() => PreviewPhase::Cancelled,
            Stage::Stopped => PreviewPhase::Failed,
        }
    }

    pub fn status(&self) -> PreviewStatus {
        self.status.clone()
    }
    pub fn clip(&self) -> &'c ClosureClip<'p> {
        self.clip
    }
    pub fn pursuit(&self) -> &SpatialPursuit {
        &self.pursuit
    }
    /// Capa `Body` dibujada: la de la persecucion en `Pursuit` (cuadro exacto de la ruta anterior), la
    /// del episodio despues (tambien en la observacion cancelada o con error).
    pub fn body_override(&self) -> Option<SolarBodyOverride<'_>> {
        match (&self.stage, &self.episode) {
            (Stage::Pursuit, _) => self.pursuit.body_override(),
            (_, Some(e)) => Some(SolarBodyOverride {
                model: &e.layer,
                extra_bounds_obj: e.bounds,
            }),
            (_, None) => None,
        }
    }
    /// Tramo (vertices, normales) de la capa dibujada; `None` sin capa.
    pub fn arm(&self) -> Option<ArmBuffers<'_>> {
        match (&self.stage, &self.episode) {
            (Stage::Pursuit, _) => {
                self.pursuit.body_override()?;
                let model = self.pursuit.composite()?;
                let (nv, nn) = self.pursuit.body_prefix()?;
                Some((&model.vertices[nv..], &model.normals[nn..]))
            }
            (_, Some(e)) => Some(e.arm()),
            (_, None) => None,
        }
    }
    pub fn reached(&self) -> Option<&ReachedPose> {
        self.episode.as_ref().map(|e| &e.reached)
    }
    /// Matrices de nave dibujada y cuerpo capturadas para el episodio vigente.
    pub fn captured_transforms(&self) -> Option<(Mat4, Mat4)> {
        self.episode.as_ref().map(|e| (e.ship, e.body))
    }
    /// Distancia (OBJ) del centroide grip de la MALLA de la capa publicada al ancla aprobada transportada
    /// con las matrices capturadas. Independiente del status del nucleo.
    pub fn final_mesh_contact_error(&self) -> Option<f64> {
        let e = self.episode.as_ref()?;
        let grip = self.clip.adapter().grip();
        let desired = self
            .clip
            .adapter()
            .anchor()
            .desired_obj(&e.ship, &e.body)
            .ok()?;
        let (v, _) = e.arm();
        let mut sum = Vec3::ZERO;
        for &i in grip {
            sum += Vec3::from_array(*v.get(i)?);
        }
        let c = sum * (1.0 / grip.len() as f64);
        let d = (c - Vec3::from_array(desired)).length();
        d.is_finite().then_some(d)
    }
    /// Centroide grip de la malla publicada y ancla transportada, en MUNDO (marcas de diagnostico).
    pub fn contact_points_world(&self, world: &World) -> Option<(Vec3, Vec3)> {
        let e = self.episode.as_ref()?;
        let c = world.creature_render_transform()?.matrix();
        let grip = self.clip.adapter().grip();
        let (v, _) = e.arm();
        let mut sum = Vec3::ZERO;
        for &i in grip {
            sum += Vec3::from_array(*v.get(i)?);
        }
        let centroid = sum * (1.0 / grip.len() as f64);
        let desired = self
            .clip
            .adapter()
            .anchor()
            .desired_obj(&e.ship, &e.body)
            .ok()?;
        Some((
            c.transform_point(centroid),
            c.transform_point(Vec3::from_array(desired)),
        ))
    }
    pub fn layer_bounds(&self) -> Option<(Vec3, f64)> {
        self.body_override().map(|o| o.extra_bounds_obj)
    }
    /// Estado para el titulo de la ventana (sin HUD sobre la imagen).
    pub fn title(&self) -> String {
        let s = &self.status;
        match s.phase {
            PreviewPhase::Pursuit => {
                let wait = if !s.armed {
                    format!("{REQUEST_KEY} pide el agarre")
                } else {
                    let p = s.pursuit;
                    let why = if p.is_none_or(|p| p.step.mode != PursuitMode::Pursuing) {
                        "esperando O2 <= 15 % y persecucion"
                    } else if !p.is_some_and(|p| p.step.reached && p.shaft.is_some()) {
                        "esperando que la punta llegue"
                    } else if s.quiet_frames < QUIET_FRAMES {
                        "esperando nave y cuerpo QUIETOS (sin tocar controles; si no llega, R vista FRONTAL y ~15 s)"
                    } else {
                        "listo"
                    };
                    format!("AGARRE PEDIDO: {why} ({REQUEST_KEY} cancela la peticion)")
                };
                format!("{} | {wait} | {RESET_KEY} reinicia", self.pursuit.title())
            }
            PreviewPhase::Handoff => {
                let h = s.handoff;
                format!(
                    "AGARRE EXPERIMENTAL: traspaso B1 {:.2}/{:.2} s | no muevas la nave (cancela) | {RESET_KEY} reinicia",
                    h.map_or(0.0, |h| h.elapsed_s),
                    h.map_or(0.0, |h| h.duration_s)
                )
            }
            PreviewPhase::Preparing | PreviewPhase::Closing | PreviewPhase::Gripped => {
                let c = s.closure;
                let name = match s.phase {
                    PreviewPhase::Preparing => "preparacion P1",
                    PreviewPhase::Closing => "cierre P1",
                    _ => "AGARRE (hold)",
                };
                format!(
                    "AGARRE EXPERIMENTAL: {name} cuadro {}/{} | contacto malla {} OBJ | no muevas la nave (cancela) | {RESET_KEY} reinicia",
                    c.map_or(0, |c| c.frame),
                    self.clip.grip_frame(),
                    self.final_mesh_contact_error()
                        .map_or("--".to_string(), |e| format!("{e:.1e}"))
                )
            }
            PreviewPhase::Cancelled => format!(
                "OBSERVACION CANCELADA ({}) en {}: ultima malla congelada, NO es contacto vigente | {RESET_KEY} reinicia",
                match s.cancel {
                    Some(CancelReason::ShipOrBodyMoved) => "la nave o el cuerpo se movieron",
                    _ => "O2 > 15 %",
                },
                phase_name(s.stopped_in)
            ),
            PreviewPhase::Failed => format!(
                "OBSERVACION con ERROR en {}: {} | {RESET_KEY} reinicia",
                phase_name(s.stopped_in),
                s.error.as_deref().unwrap_or("?")
            ),
        }
    }
}

fn closure_phase(p: ClosurePhase) -> PreviewPhase {
    match p {
        ClosurePhase::Preparing => PreviewPhase::Preparing,
        ClosurePhase::Closing => PreviewPhase::Closing,
        ClosurePhase::Gripped => PreviewPhase::Gripped,
    }
}

fn phase_name(p: Option<PreviewPhase>) -> &'static str {
    match p {
        Some(PreviewPhase::Handoff) => "traspaso B1",
        Some(PreviewPhase::Preparing) => "preparacion P1",
        Some(PreviewPhase::Closing) => "cierre P1",
        Some(PreviewPhase::Gripped) => "agarre (hold)",
        Some(PreviewPhase::Pursuit) => "persecucion",
        _ => "?",
    }
}

/// Ayuda minima (lanzador y ventana).
pub fn help_text() -> &'static str {
    "AGARRE EXPERIMENTAL (preview C, NO aprobada): persecucion espacial -> traspaso B1 -> preparacion y cierre P1.
Arranca al 100 % de O2 y no pasa nada. Pulsa 4 (objetivo 5 %) y espera unos 4 s a que baje el O2: la punta persigue la nave.
Pulsa T para pedir el agarre (puede ser antes de que llegue). Empieza cuando la punta llego y la nave esta QUIETA (sin tocar los controles).
Dura unos 8 s (1 s de traspaso + 7 s de preparacion y cierre) y se queda agarrado (hold).
Mover la nave o subir el O2 CANCELA: queda la ultima malla como OBSERVACION (no es contacto vigente).
X reinicia el experimento: la punta vuelve al reposo y persigue otra vez; la nave no se mueve. Despues, T de nuevo.
Tras volar, nave y cuerpo pueden tardar unos 20 s en quedar quietos del todo (el titulo dice que espera). Tras algunos giros la camara
y el cuerpo quedan oscilando en el ultimo decimal y no se aquietan solos: pulsa R (vista FRONTAL) y espera unos 15 s.
Controles de siempre: W/S empuje, A/D o flechas giro, Q/E alabeo, Shift impulso, 1-4 y M/N O2, 7/8/R vistas, H seguimiento, G mirada, F1 overlay, P captura, Esc sale."
}

// ---- GT1-CARRY (aditivo, solo lectura; quitar este bloque restaura C byte a byte) ----

impl<'c, 'p> GraspPreview<'c, 'p> {
    /// Cierre P1 VIVO del episodio (solo lectura); `None` fuera de preparacion/cierre/hold.
    pub fn closure(&self) -> Option<&GraspClosure<'c, 'p>> {
        match &self.stage {
            Stage::Closure(c) => Some(c),
            _ => None,
        }
    }
}

// ---- GT3-AUTO (aditivo; quitar este bloque restaura el aprobado173 byte a byte) ----

/// Alcance REAL del cuadro (politica de produccion GT3): el paso que la persecucion ACABA de publicar sobre
/// el World ya movido llego a su objetivo persiguiendo, con tramo deformado, SIN objetivo recortado (alcance
/// o bola minima) y con la punta publicada exactamente (bits) en la posicion ACTUAL de la nave del World.
/// No usa estados viejos ni proximidad a la criatura: el ancla de contacto aprobada se comprueba despues,
/// al iniciar B1 con las matrices reales (`GraspHandoff::start`).
pub fn fresh_contact(step: &SpatialFrameStatus, world: &World) -> bool {
    let ship = world.ship.position;
    let same = |a: Vec3, b: Vec3| {
        a.x.to_bits() == b.x.to_bits()
            && a.y.to_bits() == b.y.to_bits()
            && a.z.to_bits() == b.z.to_bits()
    };
    step.step.reached
        && step.step.mode == PursuitMode::Pursuing
        && !step.step.target_clamped
        && step.shaft.is_some()
        && same(step.ship_world, ship)
        && same(step.tip_world, ship)
}

impl<'c, 'p> GraspPreview<'c, 'p> {
    /// Un cuadro de la politica AUTOMATICA (GT3): igual que `frame` salvo la persecucion, que captura SIN
    /// solicitud `T` ni quietud cuando el paso de ESTE cuadro es `fresh_contact`. B1/P1, su politica de
    /// matrices capturadas (cancelacion) y el O2 no cambian; `reset` es la X de diagnostico de siempre.
    /// Con `dt` invalido, sin criatura o con identidad/capa invalidas devuelve `Err` y NADA cambia.
    pub fn frame_on_contact(
        &mut self,
        world: &World,
        dt: f64,
        reset: bool,
    ) -> Result<PreviewStatus, String> {
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}; nada cambia"
            ));
        }
        let current = study_transforms(world)?;
        if let Some(e) = &self.episode {
            let model = world
                .creature()
                .ok_or("el World no tiene criatura")?
                .model();
            let parts = world
                .staged_creature_parts()
                .ok_or("el World no tiene particion")?;
            if !same_model(&e.source_model, model) || !same_model(&e.source_body, parts.body()) {
                return Err(
                    "criatura o particion distintas del episodio capturado; nada cambia".into(),
                );
            }
            world.validate_solar_body_override(&SolarBodyOverride {
                model: &e.layer,
                extra_bounds_obj: e.bounds,
            })?;
        }
        let stable = self
            .previous
            .is_some_and(|(s, b)| same_bits(&s, &current.0) && same_bits(&b, &current.1));
        let quiet = if stable {
            self.status.quiet_frames.saturating_add(1)
        } else {
            0
        };
        let mut events = Vec::new();
        if reset {
            match self.reset(world) {
                Ok(()) => events.push(PreviewEvent::Reset),
                Err(e) => events.push(PreviewEvent::ResetRejected(e)),
            }
        }
        match std::mem::replace(&mut self.stage, Stage::Stopped) {
            Stage::Pursuit => {
                self.stage = Stage::Pursuit;
                self.contact_pursuit_frame(world, dt, current, &mut events);
            }
            Stage::Handoff(handoff) => self.handoff_frame(world, dt, handoff, current, &mut events),
            Stage::Closure(closure) => self.closure_frame(world, dt, closure, current, &mut events),
            Stage::Stopped => {}
        }
        self.previous = Some(current);
        self.status.phase = self.phase();
        self.status.events = events;
        self.status.matrices_stable = stable;
        self.status.quiet_frames = quiet;
        self.status.frame += 1;
        Ok(self.status.clone())
    }

    /// Persecucion de la politica automatica: paso con el World ya movido y, si es contacto real, la MISMA
    /// captura que `pursuit_frame` (B1 con las matrices actuales y capa copiada). Sin solicitud armada.
    fn contact_pursuit_frame(
        &mut self,
        world: &World,
        dt: f64,
        current: (Mat4, Mat4),
        events: &mut Vec<PreviewEvent>,
    ) {
        self.status.armed = false;
        let step = match self.pursuit.step(world, dt) {
            Ok(s) => s,
            Err(e) => {
                events.push(PreviewEvent::PursuitStepRejected(e));
                return;
            }
        };
        self.status.pursuit = Some(step);
        if !fresh_contact(&step, world) {
            return;
        }
        let clip: &'c ClosureClip<'p> = self.clip;
        let started = (|| {
            check_same_rig(clip, &self.pursuit)?;
            let reached = ReachedPose::capture(&self.pursuit)?;
            let handoff = GraspHandoff::start(
                clip.adapter(),
                &reached,
                &current.0,
                &current.1,
                HANDOFF_CANDIDATE_DURATION_S,
            )?;
            let layer = copy_model(self.pursuit.composite().ok_or("persecucion sin capa")?);
            let prefix = self
                .pursuit
                .body_prefix()
                .ok_or("persecucion sin prefijo")?;
            let mut episode = Episode {
                source_model: copy_model(
                    world
                        .creature()
                        .ok_or("el World no tiene criatura")?
                        .model(),
                ),
                source_body: copy_model(
                    world
                        .staged_creature_parts()
                        .ok_or("el World no tiene particion")?
                        .body(),
                ),
                ship: current.0,
                body: current.1,
                reached,
                layer,
                prefix,
                bounds: (Vec3::ZERO, 0.0),
            };
            episode.publish(handoff.vertices(), handoff.normals())?;
            Ok::<_, String>((handoff, episode))
        })();
        match started {
            Ok((handoff, episode)) => {
                self.status.handoff = Some(handoff.status());
                self.status.closure = None;
                self.status.captured_at_frame = Some(self.status.frame);
                self.episode = Some(episode);
                self.stage = Stage::Handoff(Box::new(handoff));
                events.push(PreviewEvent::Captured);
            }
            Err(e) => events.push(PreviewEvent::RequestRejected(format!(
                "contacto rechazado: {e}"
            ))),
        }
    }
}
