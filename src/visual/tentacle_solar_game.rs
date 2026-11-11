//! UMB-311 composicion OBSERVADORA del tentaculo en el Sol con ojo (debug opt-in,
//! `--tentacle-capture-debug`, tecla F7). NO es una captura jugable: no hay agarre funcional,
//! persecucion, orientacion 6DoF, bloqueo de input, audio ni reglas de ataque. Ver
//! `docs/tentacle-solar-capture.md`.
//!
//! - **Datos (privados, por argumentos):** la captura aprobada (`capture_package.txt` + OBJ), el paquete
//!   del tramo de crecimiento aprobado (`growth_package.txt`, ley `fixed`) y una correspondencia
//!   brazo->Meshy exportada fuera del repositorio. Se cargan y validan UNA vez.
//! - **Identidad:** la criatura del World debe ser el Meshy esperado (huella de geometria, conteos y
//!   bounds ORIGINALES); el brazo se identifica por procedencia verificada (1422 vertices y 2827 caras
//!   con el mismo winding, raiz = unica comparticion con el cuerpo, ojo sin interseccion). La criatura
//!   del World, su particion, el ojo y el seguimiento NO cambian.
//! - **Composicion:** UNA capa `Body` alternativa del compositor solar = particion `body` vigente sin las
//!   caras del brazo + el tramo evaluado, en espacio OBJ, con el transform, shader, textura y alfa del
//!   cuerpo (`World::render_solar_reveal_preview_with`).
//! - **F7:** toma la POSICION instantanea de la nave y el transform ACTUAL de la criatura; precalcula
//!   `o(f)` de los 312 cuadros (dominio explicito); reproduce con reloj propio (segundos reales) y el
//!   retiming PROVISIONAL `sqrt(max(1, d/d0))`; en el ultimo cuadro sostiene; otra F7 cancela.
//!   Nunca escribe nave, camara, input, O2 ni reloj del mundo.
use crate::{
    assets::obj::{Face, Model, parse_obj},
    math::{Mat4, Vec3},
    scene::world::{SolarBodyOverride, World},
    visual::tentacle_growth_cpu::{
        BaseTiming, GrowthPackage, LawSample, NUMERIC_WORLD_LIMIT, REACH_TIME_EXPONENT,
        capture::{CaptureManifest, PackageFile, fnv1a64, geometry_fnv},
    },
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Cabecera exacta de la correspondencia privada.
pub const CORRESPONDENCE_MAGIC: &str = "UMB311_ARM_CORRESPONDENCE 1";
/// Desvio maximo POR COMPONENTE entre la pose 1 de la captura y el vertice Meshy correspondiente: el
/// export OBJ de la captura tiene 6 decimales (cuanto 1e-6); medido 1.000000000001e-6. NO es bit exacto.
pub const EXPORT_POSITION_TOLERANCE: f64 = 1.5e-6;

/// Lo que pide la linea de comandos con `--tentacle-capture-debug`.
#[derive(Clone, Debug, PartialEq)]
pub struct TentacleCaptureRequest {
    pub capture_dir: PathBuf,
    pub growth_package: PathBuf,
    pub correspondence: PathBuf,
    /// Reloj inicial de la composicion activada al arrancar (`--tentacle-capture-time`, replay
    /// determinista); `None`: inactiva hasta F7.
    pub start_time: Option<f64>,
}

/// Flags crudos de la linea de comandos (para validarlos antes de cualquier IO).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TentacleCaptureFlags {
    pub enabled: bool,
    pub capture_dir: Option<PathBuf>,
    pub growth_package: Option<PathBuf>,
    pub correspondence: Option<PathBuf>,
    pub start_time: Option<f64>,
    pub solar: bool,
    pub legacy: bool,
}

/// Combinaciones de `--tentacle-capture-debug` sin tocar archivos. Sin el flag y sin sus opciones,
/// `Ok(None)` y nada cambia.
pub fn validate_tentacle_capture_flags(
    f: &TentacleCaptureFlags,
) -> Result<Option<TentacleCaptureRequest>, String> {
    let any_option = f.capture_dir.is_some()
        || f.growth_package.is_some()
        || f.correspondence.is_some()
        || f.start_time.is_some();
    if !f.enabled {
        if any_option {
            return Err("--tentacle-capture-package/--tentacle-growth-package/--tentacle-arm-correspondence/--tentacle-capture-time requieren --tentacle-capture-debug".into());
        }
        return Ok(None);
    }
    if f.legacy {
        return Err("--legacy no admite --tentacle-capture-debug".into());
    }
    if !f.solar {
        return Err("--tentacle-capture-debug requiere --creature-solar-reveal (composicion observadora en la ruta solar)".into());
    }
    let need = |v: &Option<PathBuf>, flag: &str| {
        v.clone()
            .ok_or(format!("--tentacle-capture-debug requiere {flag}"))
    };
    let request = TentacleCaptureRequest {
        capture_dir: need(
            &f.capture_dir,
            "--tentacle-capture-package <carpeta capture_v2>",
        )?,
        growth_package: need(
            &f.growth_package,
            "--tentacle-growth-package <growth_package.txt>",
        )?,
        correspondence: need(
            &f.correspondence,
            "--tentacle-arm-correspondence <arm_correspondence.txt>",
        )?,
        start_time: f.start_time,
    };
    request.validate()?;
    Ok(Some(request))
}

impl TentacleCaptureRequest {
    /// Rutas existentes del tipo esperado y reloj inicial finito >= 0 (solo metadatos, sin leer).
    pub fn validate(&self) -> Result<(), String> {
        if !self.capture_dir.is_dir() {
            return Err(format!(
                "--tentacle-capture-package no es una carpeta: {}",
                self.capture_dir.display()
            ));
        }
        for (p, flag) in [
            (&self.growth_package, "--tentacle-growth-package"),
            (&self.correspondence, "--tentacle-arm-correspondence"),
        ] {
            if !p.is_file() {
                return Err(format!("{flag} no es un archivo: {}", p.display()));
            }
        }
        if let Some(t) = self.start_time
            && !(t.is_finite() && t >= 0.0)
        {
            return Err(format!(
                "--tentacle-capture-time necesita segundos finitos >= 0: {t}"
            ));
        }
        Ok(())
    }
}

/// Correspondencia privada brazo de la captura -> Meshy (texto estricto).
#[derive(Clone, Debug, PartialEq)]
pub struct ArmCorrespondence {
    capture_fnv: u64,
    growth_fnv: u64,
    meshy_geometry_fnv: u64,
    meshy_counts: (usize, usize),
    arm_vertex: Vec<usize>,
    arm_face: Vec<usize>,
}

fn hex16(line: Option<&str>, key: &str) -> Result<u64, String> {
    let t: Vec<&str> = line.unwrap_or("").split(' ').collect();
    if t.len() != 2 || t[0] != key || t[1].len() != 16 {
        return Err(format!("correspondencia: se esperaba '{key} <16 hex>'"));
    }
    u64::from_str_radix(t[1], 16).map_err(|_| format!("correspondencia: {key} no hexadecimal"))
}

fn ids(line: Option<&str>, key: &str, n: usize) -> Result<Vec<usize>, String> {
    let t: Vec<&str> = line.unwrap_or("").split(' ').collect();
    if t.first() != Some(&key) || t.len().checked_sub(1) != Some(n) {
        return Err(format!("correspondencia: {key} con {n} indices"));
    }
    t[1..]
        .iter()
        .map(|x| {
            x.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| x.parse::<usize>().ok())
                .flatten()
                .filter(|_| !x.is_empty())
                .ok_or(format!("correspondencia: {key}: indice invalido {x:?}"))
        })
        .collect()
}

impl ArmCorrespondence {
    /// Parser ESTRICTO: LF, orden fijo, conteos exactos, indices unicos, nada tras `end`.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.contains('\r') || !text.ends_with("end\n") {
            return Err("correspondencia: saltos LF y final exacto 'end'".into());
        }
        let mut l = text.lines();
        if l.next() != Some(CORRESPONDENCE_MAGIC) {
            return Err("correspondencia: cabecera o version no soportada".into());
        }
        let capture_fnv = hex16(l.next(), "capture_package_fnv1a64")?;
        let growth_fnv = hex16(l.next(), "growth_package_fnv1a64")?;
        let meshy_geometry_fnv = hex16(l.next(), "meshy_geometry_fnv")?;
        for key in ["meshy_obj_sha256", "neutral_obj_sha256"] {
            let t: Vec<&str> = l.next().unwrap_or("").split(' ').collect();
            if t.len() != 2
                || t[0] != key
                || t[1].len() != 64
                || !t[1].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(format!("correspondencia: se esperaba '{key} <64 hex>'"));
            }
        }
        let counts = ids(l.next(), "counts", 4)?;
        let (mv, mf, av, af) = (counts[0], counts[1], counts[2], counts[3]);
        if av == 0 || af == 0 || av > mv || af > mf {
            return Err("correspondencia: conteos invalidos".into());
        }
        let arm_vertex = ids(l.next(), "arm_vertex", av)?;
        let arm_face = ids(l.next(), "arm_face", af)?;
        if l.next() != Some("end") || l.next().is_some() {
            return Err("correspondencia: falta 'end' o hay contenido tras 'end'".into());
        }
        let unique = |v: &[usize], limit: usize| {
            let set: HashSet<usize> = v.iter().copied().collect();
            set.len() == v.len() && v.iter().all(|&i| i < limit)
        };
        if !unique(&arm_vertex, mv) || !unique(&arm_face, mf) {
            return Err("correspondencia: indices repetidos o fuera del Meshy".into());
        }
        Ok(Self {
            capture_fnv,
            growth_fnv,
            meshy_geometry_fnv,
            meshy_counts: (mv, mf),
            arm_vertex,
            arm_face,
        })
    }
    pub fn arm_vertex(&self) -> &[usize] {
        &self.arm_vertex
    }
    pub fn arm_face(&self) -> &[usize] {
        &self.arm_face
    }
}

/// Inversa de la parte lineal (filas) de una matriz afin; `Err` si no es invertible o finita.
fn linear_inverse(m: &Mat4) -> Result<[Vec3; 3], String> {
    let (c0, c1, c2) = (m.column(0), m.column(1), m.column(2));
    let det = c0.dot(c1.cross(c2));
    let rows = [c1.cross(c2), c2.cross(c0), c0.cross(c1)].map(|r| r * (1.0 / det));
    if !(det.is_finite() && det != 0.0 && rows.iter().all(|r| r.is_finite())) {
        return Err("transform de la criatura no invertible o no finito".into());
    }
    Ok(rows)
}

fn in_domain(v: Vec3) -> bool {
    [v.x, v.y, v.z]
        .iter()
        .all(|c| c.is_finite() && c.abs() <= NUMERIC_WORLD_LIMIT)
}

/// Tramos base de la captura (la misma regla que la preview aceptada: `alcance` y `arrastre` con su
/// tramo siguiente y el agarre entre ambos).
pub fn capture_base_timing(m: &CaptureManifest) -> Result<BaseTiming, String> {
    let st = m.stages();
    let find = |name: &str| st.iter().position(|(_, n)| n == name);
    let (Some(i), Some(j)) = (find("alcance"), find("arrastre")) else {
        return Err("faltan los tramos 'alcance' y 'arrastre'".into());
    };
    if !(i < j && j + 1 < st.len()) {
        return Err(
            "'alcance' debe ir antes de 'arrastre' y ambos deben tener tramo siguiente".into(),
        );
    }
    let (rs, re, cs, ce) = (st[i].0, st[i + 1].0, st[j].0, st[j + 1].0);
    if !(re <= m.grip_frame() && m.grip_frame() <= cs) {
        return Err("el agarre debe caer entre el fin del alcance y el inicio del arrastre".into());
    }
    let at = |f: usize| (f as f64 - 1.0) / m.fps() as f64;
    Ok(BaseTiming {
        reach: (at(rs), at(re)),
        carry: (at(cs), at(ce)),
        duration: m.frames() as f64 / m.fps() as f64,
    })
}

/// Linea de tiempo de la composicion: la MISMA aritmetica que `GrowthPlan::sample`/`time_for_base`
/// de la preview aceptada (retiming PROVISIONAL `max(1, d/d0)^0.5` de alcance y arrastre; cuadro =
/// piso del tiempo base * fps + 1, hold en el ultimo). Separa reloj de reproduccion e indice de pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptureTimeline {
    base: BaseTiming,
    reach: (f64, f64),
    carry: (f64, f64),
    duration: f64,
    factor: f64,
    fps: usize,
    frames: usize,
}

impl CaptureTimeline {
    pub fn new(
        base: BaseTiming,
        fps: usize,
        frames: usize,
        distance: f64,
        reference_distance: f64,
    ) -> Result<Self, String> {
        if !(distance.is_finite()
            && distance >= 0.0
            && [
                base.reach.0,
                base.reach.1,
                base.carry.0,
                base.carry.1,
                base.duration,
            ]
            .iter()
            .all(|v| v.is_finite())
            && 0.0 <= base.reach.0
            && base.reach.0 < base.reach.1
            && base.reach.1 <= base.carry.0
            && base.carry.0 < base.carry.1
            && base.carry.1 <= base.duration
            && reference_distance.is_finite()
            && reference_distance > 0.0
            && fps > 0
            && frames > 0)
        {
            return Err("distancias, fps o cuadros invalidos para la linea de tiempo".into());
        }
        let factor = if distance <= reference_distance {
            1.0
        } else {
            (distance / reference_distance).powf(REACH_TIME_EXPONENT)
        };
        if !(factor.is_finite() && factor >= 1.0) {
            return Err("factor de tiempo no finito".into());
        }
        let t = base;
        let (reach, carry, duration) = if factor == 1.0 {
            (t.reach, t.carry, t.duration)
        } else {
            let ra1 = t.reach.0 + factor * (t.reach.1 - t.reach.0);
            let rc0 = ra1 + (t.carry.0 - t.reach.1);
            let rc1 = rc0 + factor * (t.carry.1 - t.carry.0);
            ((t.reach.0, ra1), (rc0, rc1), rc1 + (t.duration - t.carry.1))
        };
        if !duration.is_finite() {
            return Err("duracion no finita".into());
        }
        Ok(Self {
            base,
            reach,
            carry,
            duration,
            factor,
            fps,
            frames,
        })
    }
    pub fn factor(&self) -> f64 {
        self.factor
    }
    pub fn duration_s(&self) -> f64 {
        self.duration
    }
    pub fn reach_window(&self) -> (f64, f64) {
        self.reach
    }
    pub fn carry_window(&self) -> (f64, f64) {
        self.carry
    }
    /// `(tiempo base, cuadro 1-based)` en `seconds` de reproduccion.
    pub fn sample(&self, seconds: f64) -> Result<(f64, usize), String> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(format!("tiempo invalido (finito, >= 0): {seconds}"));
        }
        let b = &self.base;
        let ((ra0, ra1), (rc0, rc1)) = (self.reach, self.carry);
        let base_seconds = if self.factor == 1.0 || seconds < ra0 {
            seconds
        } else if seconds < ra1 {
            let u = (seconds - ra0) / (ra1 - ra0);
            b.reach.0 + u * (b.reach.1 - b.reach.0)
        } else if seconds < rc0 {
            b.reach.1 + (seconds - ra1)
        } else if seconds < rc1 {
            let u = (seconds - rc0) / (rc1 - rc0);
            b.carry.0 + u * (b.carry.1 - b.carry.0)
        } else {
            b.carry.1 + (seconds - rc1)
        };
        if !base_seconds.is_finite() {
            return Err(format!("tiempo base no finito para {seconds}"));
        }
        let f = base_seconds * self.fps as f64 + 1e-9;
        let frame = if f >= self.frames as f64 {
            self.frames
        } else {
            f.floor() as usize + 1
        };
        Ok((base_seconds, frame))
    }
}

/// Lo registrado al activar (F7): instantanea de la posicion y del transform de la criatura. Si la
/// nave o el cuerpo se mueven despues, el objetivo NO los sigue (composicion observadora).
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSnapshot {
    pub target_world: Vec3,
    pub creature_matrix: Mat4,
    pub reference_world: Vec3,
    pub destination_world: Vec3,
    pub delta_obj: [f64; 3],
    pub distance: f64,
    pub reference_distance: f64,
    pub factor: f64,
    pub duration_s: f64,
    pub cone_fraction_at_grip: Option<f64>,
    pub clamped_frames: usize,
}

/// Estado del ultimo cuadro preparado (solo salida).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptureFrameStatus {
    pub clock: f64,
    pub base_seconds: f64,
    pub frame: usize,
    pub reach_weight: f64,
    pub clamped: bool,
    pub growth_length: f64,
    /// Distancia en mundo entre el transform de la criatura de ESTE cuadro y el de la activacion
    /// aplicada al centro OBJ de la criatura (deriva del cuerpo; no es un error del agarre).
    pub creature_drift_world: f64,
}

/// Resultado de F7.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptureToggle {
    Activated(Box<CaptureSnapshot>),
    Cancelled,
}

/// Lo verificado al enlazar con la criatura del World.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BindReport {
    pub meshy_vertices: usize,
    pub meshy_faces: usize,
    pub body_faces_before: usize,
    pub arm_faces_removed: usize,
    pub eye_faces: usize,
    pub shared_root_vertices: usize,
    pub max_position_error: f64,
    pub composite_vertices: usize,
    pub composite_faces: usize,
}

pub(super) struct Bound {
    report: BindReport,
    body_vertices: usize,
    body_normals: usize,
    composite: Model,
    /// Esfera OBJ del tramo del ultimo cuadro preparado.
    extra_bounds_obj: (Vec3, f64),
}

impl Bound {
    pub(super) fn report(&self) -> BindReport {
        self.report
    }
    pub(super) fn layer(&self) -> (&Model, usize, usize) {
        (&self.composite, self.body_vertices, self.body_normals)
    }
}

struct Active {
    snapshot: CaptureSnapshot,
    laws: Vec<LawSample>,
    timeline: CaptureTimeline,
    clock: f64,
    last: Option<CaptureFrameStatus>,
}

/// Composicion observadora: paquetes validados, correspondencia, cuerpo compuesto en cache y el
/// estado de F7. Nunca escribe en el World.
pub struct SolarCaptureDebug {
    package: GrowthPackage,
    correspondence: ArmCorrespondence,
    arm_faces: Vec<[usize; 3]>,
    arm_positions: Vec<[f64; 3]>,
    growth: Model,
    bound: Option<Bound>,
    active: Option<Active>,
    creature_center_obj: Vec3,
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

impl SolarCaptureDebug {
    /// Valida TODO antes de aceptar: captura y tramo con `GrowthPackage::from_parts` (binding, ley,
    /// topologia, UV) y la correspondencia (sintaxis y vinculo FNV con ESTOS dos textos). La
    /// comprobacion contra el Meshy real ocurre en `bind`.
    pub fn from_parts(
        capture_text: &str,
        arm_frames: Vec<Model>,
        body: Model,
        growth_text: &str,
        correspondence_text: &str,
    ) -> Result<Self, String> {
        let correspondence = ArmCorrespondence::parse(correspondence_text)?;
        if correspondence.capture_fnv != fnv1a64(capture_text.as_bytes())
            || correspondence.growth_fnv != fnv1a64(growth_text.as_bytes())
        {
            return Err(
                "correspondencia vinculada a OTRA captura u OTRO paquete de crecimiento (FNV distinto)"
                    .into(),
            );
        }
        let first = arm_frames.first().ok_or("sin poses del brazo")?;
        let arm_faces: Vec<[usize; 3]> = first.faces.iter().map(|f| f.vertex_indices).collect();
        let arm_positions = first.vertices.clone();
        if arm_positions.len() != correspondence.arm_vertex.len()
            || arm_faces.len() != correspondence.arm_face.len()
        {
            return Err("correspondencia con otros conteos de brazo que la captura".into());
        }
        let package = GrowthPackage::from_parts(capture_text, arm_frames, body, growth_text)?;
        let growth = copy_model(package.data().template());
        let (c, _) = package.capture().manifest().creature_bounds();
        Ok(Self {
            package,
            correspondence,
            arm_faces,
            arm_positions,
            growth,
            bound: None,
            active: None,
            creature_center_obj: c,
        })
    }

    /// Lee la captura UNA vez (manifest y cada OBJ verificado por tamano y FNV-1a sobre los mismos
    /// bytes), el paquete de crecimiento y la correspondencia, y llama a `from_parts`.
    pub fn load(request: &TentacleCaptureRequest) -> Result<Self, String> {
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
        Self::from_parts(&text, arms, body, &growth, &correspondence)
    }

    pub fn package(&self) -> &GrowthPackage {
        &self.package
    }
    pub fn correspondence(&self) -> &ArmCorrespondence {
        &self.correspondence
    }
    pub fn bind_report(&self) -> Option<BindReport> {
        self.bound.as_ref().map(|b| b.report)
    }
    /// Cuerpo compuesto (particion `body` sin brazo + tramo) del ultimo cuadro preparado.
    pub fn composite(&self) -> Option<&Model> {
        self.bound.as_ref().map(|b| &b.composite)
    }
    /// Capa enlazada (prefijo exacto del cuerpo + plantilla del tramo) y tamanos del prefijo
    /// (vertices, normales), para rutas NUEVAS que escriben su propio tramo en una COPIA (persecucion
    /// viva). Solo lectura; `None` sin `bind`. No cambia la composicion observadora.
    pub fn bound_layer(&self) -> Option<(&Model, usize, usize)> {
        self.bound
            .as_ref()
            .map(|b| (&b.composite, b.body_vertices, b.body_normals))
    }
    /// Malla del tramo evaluada en el ultimo cuadro preparado.
    pub fn growth_mesh(&self) -> &Model {
        &self.growth
    }
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub fn snapshot(&self) -> Option<&CaptureSnapshot> {
        self.active.as_ref().map(|a| &a.snapshot)
    }
    pub fn timeline(&self) -> Option<CaptureTimeline> {
        self.active.as_ref().map(|a| a.timeline)
    }
    pub fn clock(&self) -> Option<f64> {
        self.active.as_ref().map(|a| a.clock)
    }
    pub fn last_frame(&self) -> Option<CaptureFrameStatus> {
        self.active.as_ref().and_then(|a| a.last)
    }

    /// Enlaza con la criatura del World (una vez, tras `SolarGame::start`). Verifica identidad y
    /// procedencia con los datos REALES: huella de geometria, conteos y bounds originales; cada cara
    /// del brazo = la cara Meshy indicada con el MISMO winding; posiciones a `EXPORT_POSITION_TOLERANCE`;
    /// material unico del brazo; ojo sin vertices del brazo; quitar el brazo de la particion `body`
    /// quita exactamente sus caras; la raiz de la captura es la UNICA comparticion con el cuerpo.
    /// Despues arma el cuerpo compuesto en cache. Con `Err` nada cambia.
    pub fn bind(&mut self, world: &World) -> Result<BindReport, String> {
        let prepared = self.prepare_bind(world)?;
        let report = prepared.report;
        self.publish_bind(prepared);
        Ok(report)
    }

    /// Prepare all inherited identity/composition checks without observable publication.
    pub(super) fn prepare_bind(&self, world: &World) -> Result<Bound, String> {
        let creature = world.creature().ok_or("el World no tiene criatura")?;
        let model = creature.model();
        let c = &self.correspondence;
        if (model.vertices.len(), model.faces.len()) != c.meshy_counts
            || geometry_fnv(model) != c.meshy_geometry_fnv
        {
            return Err("la criatura del World no es el Meshy de la correspondencia (huella de geometria o conteos)".into());
        }
        let manifest = self.package.capture().manifest();
        let (bc, br) = manifest.creature_bounds();
        if Vec3::from_array(model.bounds_center) != bc || model.bounds_radius != br {
            return Err("bounds de la criatura distintos de los ORIGINALES de la captura".into());
        }
        let mut max_error: f64 = 0.0;
        for (i, &m) in c.arm_vertex.iter().enumerate() {
            for a in 0..3 {
                let e = (model.vertices[m][a] - self.arm_positions[i][a]).abs();
                if e.is_nan() {
                    return Err("posicion no finita en la correspondencia".into());
                }
                max_error = max_error.max(e);
            }
        }
        if max_error > EXPORT_POSITION_TOLERANCE {
            return Err(format!(
                "brazo de la captura a {max_error:e} del Meshy (> {EXPORT_POSITION_TOLERANCE:e})"
            ));
        }
        let material = model.faces[c.arm_face[0]].material.clone();
        let mut arm_set = HashSet::with_capacity(c.arm_face.len());
        for (k, &mf) in c.arm_face.iter().enumerate() {
            let face = &model.faces[mf];
            let expected = self.arm_faces[k].map(|i| c.arm_vertex[i]);
            if face.vertex_indices != expected || face.material != material {
                return Err(format!(
                    "cara {k} del brazo: no coincide con la cara Meshy {mf} (indices, winding o material)"
                ));
            }
            arm_set.insert(expected);
        }
        let arm_vertices: HashSet<usize> = c.arm_vertex.iter().copied().collect();
        let parts = world
            .staged_creature_parts()
            .ok_or("la criatura del World no tiene particion por etapas (Sol con ojo)")?;
        let (eye, body) = (parts.eye(), parts.body());
        if eye
            .faces
            .iter()
            .any(|f| f.vertex_indices.iter().any(|v| arm_vertices.contains(v)))
        {
            return Err("el ojo usa vertices del brazo: la particion no se puede componer".into());
        }
        let kept: Vec<&Face> = body
            .faces
            .iter()
            .filter(|f| !arm_set.contains(&f.vertex_indices))
            .collect();
        let removed = body.faces.len() - kept.len();
        if removed != c.arm_face.len() {
            return Err(format!(
                "la particion body contiene {removed} caras del brazo en vez de {}",
                c.arm_face.len()
            ));
        }
        let used: HashSet<usize> = kept.iter().flat_map(|f| f.vertex_indices).collect();
        let mut shared: Vec<usize> = c
            .arm_vertex
            .iter()
            .enumerate()
            .filter(|(_, m)| used.contains(m))
            .map(|(i, _)| i)
            .collect();
        shared.sort_unstable();
        if shared != manifest.root() {
            return Err(
                "la comparticion cuerpo-brazo no es exactamente la raiz de la captura".into(),
            );
        }
        let g = self.package.data().template();
        let (nv, nuv, nn) = (body.vertices.len(), body.uvs.len(), body.normals.len());
        let mut faces: Vec<Face> = kept
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect();
        for f in &g.faces {
            let (Some(uv), Some(n)) = (f.uv_indices, f.normal_indices) else {
                return Err("tramo sin UV o normal por esquina".into());
            };
            faces.push(Face {
                vertex_indices: f.vertex_indices.map(|i| i + nv),
                uv_indices: Some(uv.map(|i| i + nuv)),
                normal_indices: Some(n.map(|i| i + nn)),
                material: material.clone(),
            });
        }
        let composite = Model {
            vertices: body.vertices.iter().chain(&g.vertices).copied().collect(),
            uvs: body.uvs.iter().chain(&g.uvs).copied().collect(),
            normals: body.normals.iter().chain(&g.normals).copied().collect(),
            faces,
            material_libraries: body.material_libraries.clone(),
            materials: body.materials.clone(),
            bounds_center: body.bounds_center,
            bounds_radius: body.bounds_radius,
        };
        let report = BindReport {
            meshy_vertices: model.vertices.len(),
            meshy_faces: model.faces.len(),
            body_faces_before: body.faces.len(),
            arm_faces_removed: removed,
            eye_faces: eye.faces.len(),
            shared_root_vertices: shared.len(),
            max_position_error: max_error,
            composite_vertices: composite.vertices.len(),
            composite_faces: composite.faces.len(),
        };
        Ok(Bound {
            report,
            body_vertices: nv,
            body_normals: nn,
            composite,
            extra_bounds_obj: (Vec3::ZERO, 0.0),
        })
    }

    /// Commit a fully prepared bind; no validation or IO after this point.
    pub(super) fn publish_bind(&mut self, prepared: Bound) {
        self.bound = Some(prepared);
    }

    /// F7: activa (instantanea) o cancela. Un `Err` de activacion no cambia nada.
    pub fn toggle(&mut self, world: &World) -> Result<CaptureToggle, String> {
        if self.active.is_some() {
            self.cancel();
            return Ok(CaptureToggle::Cancelled);
        }
        self.activate(world)
            .map(|s| CaptureToggle::Activated(Box::new(s)))
    }

    /// Cancela y vuelve a la composicion original (salto de debug declarado, sin coreografia).
    pub fn cancel(&mut self) {
        self.active = None;
    }

    /// Instantanea de la POSICION de la nave y del transform ACTUAL de la criatura; `o(f)` de los 312
    /// cuadros con la ley aceptada (dominio explicito) y la linea de tiempo. Todo antes de aceptar.
    pub fn activate(&mut self, world: &World) -> Result<CaptureSnapshot, String> {
        if self.bound.is_none() {
            return Err("composicion sin enlazar con la criatura (bind)".into());
        }
        let target = world.ship.position;
        if !in_domain(target) {
            return Err(format!(
                "posicion de la nave fuera del dominio numerico (finita, |c| <= {NUMERIC_WORLD_LIMIT:e}): {target:?}"
            ));
        }
        let c = world
            .creature_render_transform()
            .ok_or("el World no tiene criatura")?
            .matrix();
        if !c.m.iter().flatten().all(|v| v.is_finite()) {
            return Err("transform de la criatura no finito".into());
        }
        let inverse = linear_inverse(&c)?;
        let capture = self.package.capture();
        let manifest = capture.manifest();
        let reference = (c * manifest.ship_obj_matrix(1)?).column(3);
        let destination = (c * manifest.ship_obj_matrix(capture.frames())?).column(3);
        let delta_world = target - reference;
        let delta_obj = [
            inverse[0].dot(delta_world),
            inverse[1].dot(delta_world),
            inverse[2].dot(delta_world),
        ];
        let distance = (target - destination).length();
        let reference_distance = (reference - destination).length();
        if !(in_domain(reference)
            && in_domain(destination)
            && delta_obj.iter().all(|v| v.is_finite())
            && distance.is_finite())
        {
            return Err("instantanea con valores no finitos o fuera del dominio numerico".into());
        }
        let data = self.package.data();
        let laws = (1..=data.frames())
            .map(|f| data.offset(f, delta_obj))
            .collect::<Result<Vec<_>, _>>()?;
        let timeline = CaptureTimeline::new(
            capture_base_timing(manifest)?,
            data.fps(),
            data.frames(),
            distance,
            reference_distance,
        )?;
        let grip = laws[data.grip_frame() - 1];
        let snapshot = CaptureSnapshot {
            target_world: target,
            creature_matrix: c,
            reference_world: reference,
            destination_world: destination,
            delta_obj,
            distance,
            reference_distance,
            factor: timeline.factor(),
            duration_s: timeline.duration_s(),
            cone_fraction_at_grip: grip.cone_fraction,
            clamped_frames: laws.iter().filter(|l| l.clamped).count(),
        };
        self.active = Some(Active {
            snapshot: snapshot.clone(),
            laws,
            timeline,
            clock: 0.0,
            last: None,
        });
        Ok(snapshot)
    }

    /// Avanza el reloj propio con segundos REALES (no el `dt` limitado ni el reloj del mundo). Inactiva:
    /// no hace nada. `Err` (sin cambios) con segundos no finitos o negativos.
    pub fn advance(&mut self, elapsed: f64) -> Result<(), String> {
        if !(elapsed.is_finite() && elapsed >= 0.0) {
            return Err(format!("segundos reales invalidos: {elapsed}"));
        }
        if let Some(a) = &mut self.active {
            let next = a.clock + elapsed;
            if !next.is_finite() {
                return Err("reloj de la composicion no finito".into());
            }
            a.clock = next;
        }
        Ok(())
    }

    /// Pone el reloj (replay determinista con `--tentacle-capture-time`); requiere estar activa.
    pub fn set_clock(&mut self, seconds: f64) -> Result<(), String> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(format!("reloj invalido: {seconds}"));
        }
        let a = self.active.as_mut().ok_or("composicion inactiva")?;
        a.clock = seconds;
        Ok(())
    }

    /// Prepara el cuadro: inactiva -> `None` y nada cambia. Activa: evalua la ley del cuadro base en una
    /// malla temporal (finita), actualiza el cuerpo compuesto y la esfera OBJ del tramo. Con `Err` el
    /// cuerpo compuesto y el estado no cambian (el llamador puede cancelar). Despues, `body_override`.
    pub fn update_frame(&mut self, world: &World) -> Result<Option<CaptureFrameStatus>, String> {
        let Some(active) = &mut self.active else {
            return Ok(None);
        };
        let bound = self.bound.as_mut().ok_or("composicion sin enlazar")?;
        let (base_seconds, frame) = active.timeline.sample(active.clock)?;
        let law = *active
            .laws
            .get(frame - 1)
            .ok_or(format!("cuadro {frame} fuera de la ley"))?;
        // Evaluacion diferida hasta resolver las precondiciones del World.
        let current = world
            .creature_render_transform()
            .ok_or("el World no tiene criatura")?
            .matrix();
        let drift = (current.transform_point(self.creature_center_obj)
            - active
                .snapshot
                .creature_matrix
                .transform_point(self.creature_center_obj))
        .length();
        if !(current.m.iter().flatten().all(|v| v.is_finite()) && drift.is_finite()) {
            return Err("transform de la criatura o deriva no finitos".into());
        }
        // evaluate_with puede escribir parcialmente antes de Err: nunca recibe la salida publicada.
        let mut growth = copy_model(&self.growth);
        let pose = self.package.data().evaluate_with(law, &mut growth)?;
        if !growth
            .vertices
            .iter()
            .chain(&growth.normals)
            .flatten()
            .all(|v| v.is_finite())
        {
            return Err("tramo evaluado con posiciones o normales no finitas".into());
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in &growth.vertices {
            for a in 0..3 {
                lo[a] = lo[a].min(v[a]);
                hi[a] = hi[a].max(v[a]);
            }
        }
        let center = Vec3::new(
            (lo[0] + hi[0]) * 0.5,
            (lo[1] + hi[1]) * 0.5,
            (lo[2] + hi[2]) * 0.5,
        );
        let radius = growth.vertices.iter().try_fold(0.0_f64, |radius, v| {
            let distance = (Vec3::from_array(*v) - center).length();
            if !distance.is_finite() {
                return Err("distancia del tramo no finita".to_string());
            }
            Ok(radius.max(distance))
        })?;
        if !(center.is_finite() && radius.is_finite() && drift.is_finite()) {
            return Err("esfera del tramo o deriva no finitas".into());
        }
        let (nv, nn) = (bound.body_vertices, bound.body_normals);
        // Desde aqui no quedan operaciones falibles: publicar el cuadro completo junto.
        self.growth = growth;
        bound.composite.vertices[nv..].copy_from_slice(&self.growth.vertices);
        bound.composite.normals[nn..].copy_from_slice(&self.growth.normals);
        bound.extra_bounds_obj = (center, radius);
        let status = CaptureFrameStatus {
            clock: active.clock,
            base_seconds,
            frame,
            reach_weight: law.r,
            clamped: law.clamped,
            growth_length: pose.growth_length,
            creature_drift_world: drift,
        };
        active.last = Some(status);
        Ok(Some(status))
    }

    /// Capa `Body` alternativa del ultimo cuadro preparado; `None` inactiva o sin preparar (cuadro de
    /// siempre). Solo lectura.
    pub fn body_override(&self) -> Option<SolarBodyOverride<'_>> {
        let active = self.active.as_ref()?;
        active.last?;
        let bound = self.bound.as_ref()?;
        Some(SolarBodyOverride {
            model: &bound.composite,
            extra_bounds_obj: bound.extra_bounds_obj,
        })
    }

    /// Estado para el titulo de la ventana (sin HUD).
    pub fn title(&self) -> String {
        match &self.active {
            None => "F7 composicion OBSERVADORA (no agarre): inactiva".into(),
            Some(a) => {
                let frames = self.package.data().frames();
                match a.last {
                    None => format!("F7 OBSERVADORA activa | t {:.2} s", a.clock),
                    Some(s) => format!(
                        "F7 OBSERVADORA (no agarre) | cuadro {}/{frames} | t {:.2}/{:.2} s | r {:.2}{} | deriva cuerpo {:.2}",
                        s.frame,
                        s.clock,
                        a.timeline.duration_s(),
                        s.reach_weight,
                        if s.clamped { " cono" } else { "" },
                        s.creature_drift_world
                    ),
                }
            }
        }
    }
}
