//! UMB-311 preview CPU AISLADA: el brazo real se estira, agarra la nave REAL y la trae bajo el ojo de la
//! criatura REAL. Reproduce en el mundo Rust el estudio Blender CAP v2 (312 poses horneadas a 24 fps,
//! 13 s) desde un paquete privado (`capture_package.txt` + OBJ). No es la captura del juego: es un
//! escenario aislado colocado en alcance (la nave nace a ~84 del centro, no en el spawn de 200).
//! Ver `docs/tentacle-capture-cpu-preview.md`.
//!
//! - Todo vive en el espacio OBJ de la criatura y se dibuja con `CreaturePlacement::demo().transform_for`
//!   usando los bounds del MONSTRUO ORIGINAL guardados en el paquete (el cuerpo recortado tiene otros).
//! - Muestreo por cuadro (piso de `t * fps`), sin interpolacion: malla y nave usan SIEMPRE el mismo
//!   cuadro, asi que la relacion agarre-nave no puede deslizar entre cuadros. Sin bucle: clamp al final.
use std::path::Path;
use umbral::{
    assets::obj::{Model, parse_obj},
    math::{Mat4, Transform, Vec3},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        pipeline::draw_model_with,
        shader::Material,
        stats::RenderStats,
        texture::Texture,
    },
    scene::world::{BASE_FOV, SHIP_MODEL_YAW, SPACE_COLOR, World},
    visual::{
        perception_game::apply_lucid_perception,
        static_creature::{AMBIENT, CreaturePlacement},
        textured::{TexturedMode, TexturedShader},
    },
};

/// Cabecera exacta del paquete.
pub const CAPTURE_PACKAGE_MAGIC: &str = "UMB311_CAPTURE_PACKAGE 1";
/// Tolerancia de la cadena de transforms de la nave en mundo (las matrices de Blender son f32).
pub const SHIP_CHAIN_TOLERANCE: f64 = 1e-5;
/// Desvio maximo de ortogonalidad/escala aceptado en una matriz de nave del paquete (f32).
pub const SHIP_MATRIX_TOLERANCE: f64 = 1e-5;
/// Guarda contra el redondeo de `t * fps` justo en un borde de cuadro.
const FRAME_EPSILON: f64 = 1e-9;
/// Distancia al objetivo de la camara de presentacion (solo define la direccion).
const PRESENTATION_TARGET_DISTANCE: f64 = 100.0;

/// FNV-1a de 64 bits (huella de los archivos del paquete; no criptografica).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Huella de la GEOMETRIA de un modelo (FNV-1a sobre conteo y bits f64 de vertices, conteo e indices
/// de caras, todo little-endian u64): identifica la nave del World frente a la del paquete.
pub fn geometry_fnv(model: &Model) -> u64 {
    let mut bytes = Vec::with_capacity(16 + model.vertices.len() * 24 + model.faces.len() * 24);
    bytes.extend_from_slice(&(model.vertices.len() as u64).to_le_bytes());
    for v in &model.vertices {
        for c in v {
            bytes.extend_from_slice(&c.to_bits().to_le_bytes());
        }
    }
    bytes.extend_from_slice(&(model.faces.len() as u64).to_le_bytes());
    for f in &model.faces {
        for i in f.vertex_indices {
            bytes.extend_from_slice(&(i as u64).to_le_bytes());
        }
    }
    fnv1a64(&bytes)
}

/// Archivo del paquete con tamano y huella.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageFile {
    pub relative: String,
    pub bytes: u64,
    pub fnv1a64: u64,
}

/// Camara de presentacion inspeccionada en Blender, en espacio OBJ de la criatura.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationCamera {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub fov_y: f64,
}

/// Contenido validado de `capture_package.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureManifest {
    fps: usize,
    frames: usize,
    arm_counts: (usize, usize),
    body_counts: (usize, usize),
    grip_frame: usize,
    creature_center: Vec3,
    creature_radius: f64,
    ship_center: Vec3,
    ship_radius: f64,
    ship_geometry: u64,
    camera: PresentationCamera,
    stages: Vec<(usize, String)>,
    root: Vec<usize>,
    grip: Vec<usize>,
    body_file: PackageFile,
    arm_files: Vec<PackageFile>,
    ships: Vec<Mat4>,
}

fn arr(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

fn finite_mat(m: &Mat4) -> bool {
    m.m.iter().flatten().all(|v| v.is_finite())
}

fn numbers(tokens: &[&str], what: &str) -> Result<Vec<f64>, String> {
    tokens
        .iter()
        .map(|t| {
            t.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or(format!("{what}: numero invalido {t:?}"))
        })
        .collect()
}

fn count(token: &str, what: &str) -> Result<usize, String> {
    token
        .parse::<usize>()
        .map_err(|_| format!("{what}: entero invalido {token:?}"))
}

fn index_list(tokens: &[&str], what: &str, limit: usize) -> Result<Vec<usize>, String> {
    let (n, rest) = tokens.split_first().ok_or(format!("{what}: vacio"))?;
    let n = count(n, what)?;
    if n == 0 || rest.len() != n {
        return Err(format!(
            "{what}: se esperaban {n} indices y hay {}",
            rest.len()
        ));
    }
    let mut out: Vec<usize> = rest
        .iter()
        .map(|t| count(t, what))
        .collect::<Result<_, _>>()?;
    if out.iter().any(|&i| i >= limit) {
        return Err(format!(
            "{what}: indice fuera de la malla de {limit} vertices"
        ));
    }
    out.sort_unstable();
    if out.windows(2).any(|w| w[0] == w[1]) {
        return Err(format!("{what}: indices repetidos"));
    }
    Ok(out)
}

/// Escala uniforme de la parte lineal de `m` (columnas ortogonales de igual norma) o `Err`.
fn uniform_scale(m: &Mat4, tolerance: f64) -> Result<f64, String> {
    let c = [m.column(0), m.column(1), m.column(2)];
    let s = c[0].length();
    let ok = s.is_finite()
        && s > 0.0
        && c.iter().all(|v| ((v.length() - s) / s).abs() <= tolerance)
        && (c[0].dot(c[1]) / (s * s)).abs() <= tolerance
        && (c[0].dot(c[2]) / (s * s)).abs() <= tolerance
        && (c[1].dot(c[2]) / (s * s)).abs() <= tolerance
        && c[0].cross(c[1]).dot(c[2]) > 0.0
        && m.m[3] == [0.0, 0.0, 0.0, 1.0];
    if ok {
        Ok(s)
    } else {
        Err("matriz no rigida (ortogonalidad, escala uniforme o mano)".into())
    }
}

impl CaptureManifest {
    /// Parser ESTRICTO: orden fijo de lineas, conteos exactos, sin tokens extra ni nada tras `end`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        let mut next = |key: &str| -> Result<Vec<String>, String> {
            let line = lines.next().ok_or(format!("falta la linea {key:?}"))?;
            let toks: Vec<&str> = line.split(' ').collect();
            if toks.first() != Some(&key) || toks.iter().any(|t| t.is_empty()) {
                return Err(format!("se esperaba {key:?} y llego {line:?}"));
            }
            Ok(toks[1..].iter().map(|s| s.to_string()).collect())
        };
        let head = next("UMB311_CAPTURE_PACKAGE")?;
        if head != ["1"] {
            return Err(format!("version de paquete no soportada: {head:?}"));
        }
        let one = |v: Vec<String>, what: &str| -> Result<usize, String> {
            if v.len() != 1 {
                return Err(format!("{what}: se esperaba un valor"));
            }
            count(&v[0], what)
        };
        let fps = one(next("fps")?, "fps")?;
        let frames = one(next("frames")?, "frames")?;
        if !(1..=240).contains(&fps) || !(2..=100_000).contains(&frames) {
            return Err(format!("fps {fps} o frames {frames} fuera de rango"));
        }
        let pair = |v: Vec<String>, what: &str| -> Result<(usize, usize), String> {
            if v.len() != 2 {
                return Err(format!("{what}: se esperaban 2 valores"));
            }
            let (a, b) = (count(&v[0], what)?, count(&v[1], what)?);
            if a == 0 || b == 0 {
                return Err(format!("{what}: conteo nulo"));
            }
            Ok((a, b))
        };
        let arm_counts = pair(next("arm_counts")?, "arm_counts")?;
        let body_counts = pair(next("body_counts")?, "body_counts")?;
        let grip_frame = one(next("grip_frame")?, "grip_frame")?;
        if !(2..=frames).contains(&grip_frame) {
            return Err(format!("grip_frame {grip_frame} fuera de [2, {frames}]"));
        }
        let sphere = |v: Vec<String>, what: &str| -> Result<(Vec3, f64), String> {
            let t: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
            let n = numbers(&t, what)?;
            if n.len() != 4 || n[3] <= 0.0 {
                return Err(format!("{what}: centro + radio > 0"));
            }
            Ok((Vec3::new(n[0], n[1], n[2]), n[3]))
        };
        let (creature_center, creature_radius) =
            sphere(next("creature_bounds")?, "creature_bounds")?;
        let (ship_center, ship_radius) = sphere(next("ship_bounds")?, "ship_bounds")?;
        let geo = next("ship_geometry")?;
        let ship_geometry = (geo.len() == 1 && geo[0].len() == 16)
            .then(|| u64::from_str_radix(&geo[0], 16).ok())
            .flatten()
            .ok_or("ship_geometry: huella hexadecimal de 16 cifras")?;
        let cam = next("camera")?;
        let t: Vec<&str> = cam.iter().map(|s| s.as_str()).collect();
        let n = numbers(&t, "camera")?;
        if n.len() != 10 {
            return Err("camera: 10 valores (posicion, adelante, arriba, fov)".into());
        }
        let camera = PresentationCamera {
            position: Vec3::new(n[0], n[1], n[2]),
            forward: Vec3::new(n[3], n[4], n[5]),
            up: Vec3::new(n[6], n[7], n[8]),
            fov_y: n[9],
        };
        let f = camera.forward.length();
        let u = camera.up.length();
        if !(f > 1e-9 && u > 1e-9 && camera.forward.cross(camera.up).length() > 1e-6 * f * u)
            || !(camera.fov_y > 0.0 && camera.fov_y < std::f64::consts::PI)
        {
            return Err("camera: base degenerada o fov fuera de (0, pi)".into());
        }
        let mut rest: Vec<&str> = lines.collect();
        let mut stages = Vec::new();
        while rest.first().is_some_and(|l| l.starts_with("stage ")) {
            let toks: Vec<&str> = rest.remove(0).split(' ').collect();
            let ok_name = toks.len() == 3
                && !toks[2].is_empty()
                && toks[2].bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
            if !ok_name {
                return Err(format!("stage invalido: {toks:?}"));
            }
            let start = count(toks[1], "stage")?;
            let increasing = stages
                .last()
                .is_none_or(|(s, _): &(usize, String)| start > *s);
            if !(1..=frames).contains(&start) || !increasing || (stages.is_empty() && start != 1) {
                return Err(format!("stage {start} fuera de orden o de rango"));
            }
            stages.push((start, toks[2].to_string()));
        }
        if stages.is_empty() {
            return Err("faltan stages".into());
        }
        let mut take = |key: &str| -> Result<Vec<String>, String> {
            let line = if rest.is_empty() {
                None
            } else {
                Some(rest.remove(0))
            };
            let line = line.ok_or(format!("falta la linea {key:?}"))?;
            let toks: Vec<&str> = line.split(' ').collect();
            if toks.first() != Some(&key) || toks.iter().any(|t| t.is_empty()) {
                return Err(format!("se esperaba {key:?} y llego {line:?}"));
            }
            Ok(toks[1..].iter().map(|s| s.to_string()).collect())
        };
        let root_t = take("root")?;
        let root = index_list(
            &root_t.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "root",
            arm_counts.0,
        )?;
        let grip_t = take("grip")?;
        let grip = index_list(
            &grip_t.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "grip",
            arm_counts.0,
        )?;
        let file = |v: Vec<String>, kind: &str, name: &str| -> Result<PackageFile, String> {
            if v.len() != 4 || v[0] != kind || v[1] != name {
                return Err(format!("se esperaba file {kind} {name}: {v:?}"));
            }
            let bytes = v[2]
                .parse::<u64>()
                .map_err(|_| format!("bytes invalidos en {name}"))?;
            let hash = (v[3].len() == 16)
                .then(|| u64::from_str_radix(&v[3], 16).ok())
                .flatten()
                .ok_or(format!("huella invalida en {name}"))?;
            Ok(PackageFile {
                relative: name.to_string(),
                bytes,
                fnv1a64: hash,
            })
        };
        let body_file = file(take("file")?, "body", "body/cuerpo_presentacion.obj")?;
        let mut arm_files = Vec::with_capacity(frames);
        for k in 1..=frames {
            arm_files.push(file(take("file")?, "arm", &format!("arm/brazo{k:04}.obj"))?);
        }
        let mut ships = Vec::with_capacity(frames);
        for k in 1..=frames {
            let v = take("ship")?;
            if v.len() != 13 || count(&v[0], "ship")? != k {
                return Err(format!("ship {k}: cuadro y 12 valores"));
            }
            let t: Vec<&str> = v[1..].iter().map(|s| s.as_str()).collect();
            let n = numbers(&t, "ship")?;
            let mut m = Mat4::IDENTITY;
            for r in 0..3 {
                for c in 0..4 {
                    m.m[r][c] = n[r * 4 + c];
                }
            }
            uniform_scale(&m, SHIP_MATRIX_TOLERANCE).map_err(|e| format!("ship {k}: {e}"))?;
            ships.push(m);
        }
        if ships[..grip_frame - 1].iter().any(|m| *m != ships[0]) {
            return Err("la nave se mueve antes del cuadro de agarre".into());
        }
        if take("end")? != Vec::<String>::new() || !rest.is_empty() {
            return Err("contenido tras 'end'".into());
        }
        Ok(Self {
            fps,
            frames,
            arm_counts,
            body_counts,
            grip_frame,
            creature_center,
            creature_radius,
            ship_center,
            ship_radius,
            ship_geometry,
            camera,
            stages,
            root,
            grip,
            body_file,
            arm_files,
            ships,
        })
    }
    pub fn fps(&self) -> usize {
        self.fps
    }
    pub fn frames(&self) -> usize {
        self.frames
    }
    pub fn grip_frame(&self) -> usize {
        self.grip_frame
    }
    pub fn creature_bounds(&self) -> (Vec3, f64) {
        (self.creature_center, self.creature_radius)
    }
    pub fn ship_bounds(&self) -> (Vec3, f64) {
        (self.ship_center, self.ship_radius)
    }
    /// Huella de geometria de la nave del repositorio con la que se horneo el paquete.
    pub fn ship_geometry(&self) -> u64 {
        self.ship_geometry
    }
    pub fn camera(&self) -> PresentationCamera {
        self.camera
    }
    pub fn stages(&self) -> &[(usize, String)] {
        &self.stages
    }
    pub fn root(&self) -> &[usize] {
        &self.root
    }
    pub fn grip(&self) -> &[usize] {
        &self.grip
    }
    pub fn files(&self) -> impl Iterator<Item = &PackageFile> {
        std::iter::once(&self.body_file).chain(self.arm_files.iter())
    }
    /// Matriz de la nave (modelo de la nave -> OBJ de la criatura) del cuadro 1-based.
    pub fn ship_obj_matrix(&self, frame: usize) -> Result<Mat4, String> {
        frame
            .checked_sub(1)
            .and_then(|i| self.ships.get(i))
            .copied()
            .ok_or(format!("cuadro {frame} fuera de 1..={}", self.frames))
    }
}

/// Pose horneada del brazo (posiciones y normales por esquina de un cuadro).
#[derive(Clone, Debug)]
struct ArmPose {
    positions: Vec<[f64; 3]>,
    normals: Vec<[f64; 3]>,
    normal_indices: Vec<[usize; 3]>,
}

fn validate_model(model: &Model, what: &str) -> Result<(), String> {
    if model.vertices.iter().flatten().any(|c| !c.is_finite())
        || model.uvs.iter().flatten().any(|c| !c.is_finite())
        || model.normals.iter().flatten().any(|c| !c.is_finite())
    {
        return Err(format!("{what}: valores no finitos"));
    }
    for f in &model.faces {
        if f.vertex_indices.iter().any(|&i| i >= model.vertices.len())
            || f.uv_indices
                .is_some_and(|ids| ids.iter().any(|&i| i >= model.uvs.len()))
            || f.normal_indices
                .is_some_and(|ids| ids.iter().any(|&i| i >= model.normals.len()))
        {
            return Err(format!(
                "{what}: cara con indice de vertice, UV o normal fuera de rango"
            ));
        }
    }
    Ok(())
}

/// Paquete validado: manifest + poses del brazo + cuerpo de presentacion. Campos privados e inmutables.
pub struct CapturePackage {
    manifest: CaptureManifest,
    /// Malla de trabajo del brazo (pose 1 al cargar); solo la muta la `CapturePreview` propietaria.
    arm: Model,
    poses: Vec<ArmPose>,
    body: Model,
    creature: Transform,
}

impl CapturePackage {
    /// Valida todo antes de aceptar: conteos, topologia y UV por esquina identicas a la pose 1,
    /// normales por esquina finitas y no degeneradas (se renormalizan), cuerpo con los conteos del
    /// manifest y transform de la criatura con los bounds ORIGINALES del manifest.
    pub fn from_parts(
        manifest_text: &str,
        arm_frames: Vec<Model>,
        body: Model,
    ) -> Result<Self, String> {
        let manifest = CaptureManifest::parse(manifest_text)?;
        if arm_frames.len() != manifest.frames {
            return Err(format!(
                "{} poses para {} cuadros",
                arm_frames.len(),
                manifest.frames
            ));
        }
        let (nv, nf) = manifest.arm_counts;
        let first = &arm_frames[0];
        let mut poses = Vec::with_capacity(arm_frames.len());
        for (k, m) in arm_frames.iter().enumerate() {
            let what = format!("pose {}", k + 1);
            validate_model(m, &what)?;
            if m.vertices.len() != nv || m.faces.len() != nf {
                return Err(format!(
                    "{what}: {}/{} en vez de {nv}/{nf}",
                    m.vertices.len(),
                    m.faces.len()
                ));
            }
            let mut normal_indices = Vec::with_capacity(nf);
            for (a, b) in m.faces.iter().zip(&first.faces) {
                let (Some(ua), Some(ub), Some(na)) = (a.uv_indices, b.uv_indices, a.normal_indices)
                else {
                    return Err(format!("{what}: cara sin UV o sin normal por esquina"));
                };
                let same_uv = (0..3).all(|c| m.uvs[ua[c]] == first.uvs[ub[c]]);
                if a.vertex_indices != b.vertex_indices || !same_uv {
                    return Err(format!("{what}: topologia o UV distintas de la pose 1"));
                }
                normal_indices.push(na);
            }
            let mut normals = Vec::with_capacity(m.normals.len());
            for n in &m.normals {
                let v = Vec3::from_array(*n);
                let l = v.length();
                if !(0.9..=1.1).contains(&l) {
                    return Err(format!("{what}: normal degenerada (|n| = {l})"));
                }
                normals.push(arr(v * (1.0 / l)));
            }
            poses.push(ArmPose {
                positions: m.vertices.clone(),
                normals,
                normal_indices,
            });
        }
        validate_model(&body, "cuerpo")?;
        if (body.vertices.len(), body.faces.len()) != manifest.body_counts {
            return Err("cuerpo: conteos distintos del manifest".into());
        }
        let (c, r) = manifest.creature_bounds();
        let creature = CreaturePlacement::demo().transform_for(c, r)?;
        let mut arm = arm_frames.into_iter().next().ok_or("sin poses")?;
        arm.normals = poses[0].normals.clone();
        Ok(Self {
            manifest,
            arm,
            poses,
            body,
            creature,
        })
    }

    /// Lee el paquete UNA vez: manifest, y cada OBJ verificado por tamano y FNV-1a sobre los MISMOS
    /// bytes que luego se parsean (sin releer). Un archivo ajeno o tocado da `Err`.
    pub fn load(dir: &Path) -> Result<Self, String> {
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
        let body = read(&manifest.body_file)?;
        let arms = manifest
            .arm_files
            .iter()
            .map(read)
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_parts(&text, arms, body)
    }
    pub fn manifest(&self) -> &CaptureManifest {
        &self.manifest
    }
    pub fn frames(&self) -> usize {
        self.manifest.frames
    }
    pub fn fps(&self) -> usize {
        self.manifest.fps
    }
    /// Duracion del clip: `frames / fps` (13 s en el paquete real).
    pub fn duration_s(&self) -> f64 {
        self.manifest.frames as f64 / self.manifest.fps as f64
    }
    /// Cuadro 1-based mostrado en `seconds`: piso de `t * fps`, clamp al ultimo (hold, sin bucle).
    pub fn frame_at(&self, seconds: f64) -> Result<usize, String> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(format!(
                "tiempo de captura invalido (finito, >= 0): {seconds}"
            ));
        }
        let f = seconds * self.manifest.fps as f64 + FRAME_EPSILON;
        let last = self.manifest.frames;
        Ok(if f >= last as f64 {
            last
        } else {
            f.floor() as usize + 1
        })
    }
    /// Tramo del cuadro 1-based.
    pub fn stage_at(&self, frame: usize) -> Result<&str, String> {
        if !(1..=self.manifest.frames).contains(&frame) {
            return Err(format!(
                "cuadro {frame} fuera de 1..={}",
                self.manifest.frames
            ));
        }
        Ok(self
            .manifest
            .stages
            .iter()
            .rev()
            .find(|(s, _)| *s <= frame)
            .map(|(_, n)| n.as_str())
            .unwrap_or("reposo"))
    }
    /// Transform de dibujo de cuerpo y brazo: el de la criatura demo con los bounds ORIGINALES.
    pub fn creature_transform(&self) -> Transform {
        self.creature
    }
    pub fn body(&self) -> &Model {
        &self.body
    }
    /// Copia en sitio la pose horneada del cuadro 1-based (ya validado) en la malla de trabajo.
    fn pose_arm(&mut self, frame: usize) {
        let p = &self.poses[frame - 1];
        self.arm.vertices.clone_from(&p.positions);
        self.arm.normals.clone_from(&p.normals);
        for (face, n) in self.arm.faces.iter_mut().zip(&p.normal_indices) {
            face.normal_indices = Some(*n);
        }
    }
    /// Matriz de mundo de la nave en el cuadro: transform de la criatura * matriz OBJ del paquete.
    pub fn ship_world_matrix(&self, frame: usize) -> Result<Mat4, String> {
        Ok(self.creature.matrix() * self.manifest.ship_obj_matrix(frame)?)
    }
    /// Centro del ojo (`CREATURE_EYE_REGION`, coordenadas OBJ) en mundo.
    pub fn eye_world(&self, eye_obj: Vec3) -> Vec3 {
        self.creature.matrix().transform_point(eye_obj)
    }
}

/// Vista de la preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureView {
    /// Camara de presentacion inspeccionada en Blender (por defecto).
    Presentation,
    /// CANDIDATA: seguimiento con `chase_distance`/`chase_height` del World y la formula de
    /// `snap_camera` (sin suavizado). No es seleccion de camara del juego.
    ChaseCandidate,
}

/// Lo que se aplico/dibujo en un cuadro (solo salida).
#[derive(Clone, Copy, Debug)]
pub struct CaptureFrame {
    pub frame: usize,
    pub seconds: f64,
    pub ship_position: Vec3,
    pub stats: RenderStats,
}

/// Preview: posee el paquete, la textura y la malla de trabajo del brazo (pose en sitio). Sin estado
/// publico mutable; el World recibe nave y camara del MISMO cuadro en cada llamada.
pub struct CapturePreview {
    package: CapturePackage,
    texture: Texture,
    posed: usize,
}

struct Prepared {
    frame: usize,
    ship_position: Vec3,
    ship_orientation: Mat4,
    camera: umbral::scene::camera::Camera,
}

impl CapturePreview {
    pub fn new(package: CapturePackage, texture: Texture) -> Self {
        Self {
            package,
            texture,
            posed: 1,
        }
    }
    pub fn package(&self) -> &CapturePackage {
        &self.package
    }
    /// Malla del brazo tal como se dibujo/aplico por ultima vez (solo lectura).
    pub fn posed_arm(&self) -> &Model {
        &self.package.arm
    }
    pub fn posed_frame(&self) -> usize {
        self.posed
    }

    fn pose(&mut self, frame: usize) {
        self.package.pose_arm(frame);
        self.posed = frame;
    }

    /// Valida TODO (tiempo, World, nave del World, cuadro, matriz, camara) sin tocar nada.
    fn prepare(
        &self,
        world: &World,
        width: usize,
        height: usize,
        seconds: f64,
        view: CaptureView,
    ) -> Result<Prepared, String> {
        let frame = self.package.frame_at(seconds)?;
        if !(0.0..=1.0).contains(&world.debug_oxygen01) {
            return Err(format!(
                "O2 del mundo fuera de [0, 1]: {}",
                world.debug_oxygen01
            ));
        }
        if !(world.time.is_finite() && world.time >= 0.0) {
            return Err(format!("tiempo del mundo invalido: {}", world.time));
        }
        if world.creature().is_some() {
            return Err("el World ya tiene una criatura: se dibujarian dos cuerpos".into());
        }
        let count = width
            .checked_mul(height)
            .filter(|&n| n > 0 && n <= MAX_PIXELS);
        if count.is_none() {
            return Err(format!("cuadro invalido: {width}x{height}"));
        }
        let (sc, sr) = self.package.manifest.ship_bounds();
        let ship = &world.ship_model;
        if Vec3::from_array(ship.bounds_center) != sc
            || ship.bounds_radius != sr
            || geometry_fnv(ship) != self.package.manifest.ship_geometry
        {
            return Err(
                "la nave del World no es la nave del paquete (bounds o geometria distintos)".into(),
            );
        }
        validate_model(ship, "nave del World")?;
        let w = self.package.ship_world_matrix(frame)?;
        if !finite_mat(&w) {
            return Err("matriz de mundo de la nave no finita".into());
        }
        let s = uniform_scale(&w, SHIP_CHAIN_TOLERANCE)?;
        if (s - 1.0).abs() > SHIP_CHAIN_TOLERANCE {
            return Err(format!("la nave quedaria escalada en mundo: {s}"));
        }
        let rotation = w.orthonormalized() * Mat4::rotation_y(-SHIP_MODEL_YAW);
        let mut orientation = rotation.orthonormalized();
        orientation.m[3] = [0.0, 0.0, 0.0, 1.0];
        let ship_position = w.column(3);
        if !ship_position.is_finite() {
            return Err("posicion de la nave no finita".into());
        }
        let mut probe = world.ship;
        probe.position = ship_position;
        probe.orientation = orientation;
        let drawn = probe.transform(SHIP_MODEL_YAW).matrix();
        if !finite_mat(&drawn) {
            return Err("matriz dibujada de la nave no finita".into());
        }
        for (r, row) in drawn.m.iter().enumerate().take(3) {
            for (c, &value) in row.iter().enumerate() {
                let difference = (value - w.m[r][c]).abs();
                if !difference.is_finite() || difference > SHIP_CHAIN_TOLERANCE {
                    return Err(format!(
                        "cadena de transforms de la nave rota: {difference}"
                    ));
                }
            }
        }
        let mut camera = world.camera;
        let (near, far) = (world.camera.near, world.camera.far);
        let aspect = width as f64 / height as f64;
        match view {
            CaptureView::Presentation => {
                let pc = self.package.manifest.camera();
                let c = self.package.creature.matrix();
                camera.position = c.transform_point(pc.position);
                let forward = c.transform_dir(pc.forward).normalized();
                camera.target = camera.position + forward * PRESENTATION_TARGET_DISTANCE;
                camera.up = c.transform_dir(pc.up).normalized();
                camera.set_lens(pc.fov_y, aspect, near, far)?;
            }
            CaptureView::ChaseCandidate => {
                let (d, h) = (world.chase_distance, world.chase_height);
                if !(d.is_finite() && h.is_finite()) {
                    return Err("chase_distance/chase_height no finitos".into());
                }
                let back = orientation.transform_dir(Vec3::Z);
                let up = orientation.transform_dir(Vec3::Y);
                camera.position = ship_position + back * d + up * h;
                camera.target = ship_position - back * 30.0;
                camera.up = up;
                camera.set_lens(BASE_FOV, aspect, near, far)?;
            }
        }
        let vp = camera.view_projection();
        if !(camera.position.is_finite() && camera.target.is_finite() && camera.up.is_finite())
            || (camera.target - camera.position).length() <= 0.0
            || !finite_mat(&vp)
        {
            return Err("camara resultante no usable".into());
        }
        Ok(Prepared {
            frame,
            ship_position,
            ship_orientation: orientation,
            camera,
        })
    }

    /// Aplica el cuadro de `seconds` al World (nave y camara) y a la malla de trabajo, sin dibujar.
    /// Valida todo antes: con `Err` nada cambia.
    pub fn apply(
        &mut self,
        world: &mut World,
        width: usize,
        height: usize,
        seconds: f64,
        view: CaptureView,
    ) -> Result<CaptureFrame, String> {
        let p = self.prepare(world, width, height, seconds, view)?;
        self.pose(p.frame);
        world.ship.position = p.ship_position;
        world.ship.orientation = p.ship_orientation;
        world.camera = p.camera;
        Ok(CaptureFrame {
            frame: p.frame,
            seconds,
            ship_position: p.ship_position,
            stats: RenderStats::default(),
        })
    }

    /// Cuadro completo en el World REAL: World (estrellas, cuerpos, nave del cuadro) -> cuerpo de
    /// presentacion -> brazo -> percepcion aprobada AL FINAL si `perception`. Se dibuja en un cuadro
    /// temporal y solo se intercambia si todo salio bien: un `Err` deja pixeles, profundidad, nave y
    /// camara intactos. `with_arm = false` da la referencia cuerpo+nave sin brazo.
    pub fn render(
        &mut self,
        world: &mut World,
        framebuffer: &mut Framebuffer,
        seconds: f64,
        view: CaptureView,
        perception: bool,
        with_arm: bool,
    ) -> Result<CaptureFrame, String> {
        let (w, h) = (framebuffer.width, framebuffer.height);
        let n = w
            .checked_mul(h)
            .filter(|&n| n > 0 && n <= MAX_PIXELS)
            .ok_or("cuadro invalido")?;
        if framebuffer.pixels.len() != n {
            return Err(format!(
                "el cuadro tiene {} pixeles para {w}x{h}",
                framebuffer.pixels.len()
            ));
        }
        let p = self.prepare(world, w, h, seconds, view)?;
        let shader = TexturedShader::new(
            &self.texture,
            TexturedMode::Lit,
            world.system.light_position(),
            AMBIENT,
        )?;
        let mut scratch = Framebuffer::new(w, h, SPACE_COLOR)?;
        let saved = (world.ship, world.camera);
        let previous_pose = self.posed;
        self.package.pose_arm(p.frame);
        self.posed = p.frame;
        world.ship.position = p.ship_position;
        world.ship.orientation = p.ship_orientation;
        world.camera = p.camera;
        let mut stats = world.render(&mut scratch);
        let material = Material::from_model();
        let creature = self.package.creature;
        let mut models: Vec<&Model> = vec![&self.package.body];
        if with_arm {
            models.push(&self.package.arm);
        }
        for model in models {
            let d = draw_model_with(
                &mut scratch,
                model,
                &creature,
                &material,
                &world.camera,
                |f, _| shader.shade(f),
            );
            stats += d;
        }
        if perception && let Err(e) = apply_lucid_perception(&mut scratch, world) {
            (world.ship, world.camera) = saved;
            self.package.pose_arm(previous_pose);
            self.posed = previous_pose;
            return Err(e);
        }
        std::mem::swap(framebuffer, &mut scratch);
        Ok(CaptureFrame {
            frame: p.frame,
            seconds,
            ship_position: p.ship_position,
            stats,
        })
    }
}
