//! Mirada local del ojo de la criatura (nucleo puro, sin `World`): calibracion en reposo,
//! objetivo, limites, estado temporal y pose en cache. Ver `docs/static-creature.md`.

use crate::{
    assets::obj::{Face, Model},
    math::{Mat4, Vec3},
    visual::{staged_creature_reveal::EyeRegion, static_creature::StaticCreature},
};
use std::sync::Arc;

/// Combinaciones de `--creature-eye-follow`, sin tocar archivos. Se valida despues de
/// `validate_staged_flags`: con los dos modos de revelacion gana su error.
pub fn validate_eye_follow_flags(
    eye_follow: bool,
    stages: bool,
    has_creature: bool,
    legacy: bool,
) -> Result<(), String> {
    if !eye_follow {
        return Ok(());
    }
    if legacy {
        return Err("--legacy no admite --creature-eye-follow".into());
    }
    if !stages {
        return Err("--creature-eye-follow necesita --creature-reveal-stages".into());
    }
    if !has_creature {
        return Err("--creature-eye-follow necesita --creature-model y --creature-texture".into());
    }
    Ok(())
}

const DEGREE: f64 = std::f64::consts::PI / 180.0;
/// Limites de la mirada (radianes): yaw +-10 grados y pitch +-8 grados, las poses de V02
/// aprobadas (cada eje por separado). La envolvente **elipse**
/// `(yaw / MAX_YAW)^2 + (pitch / MAX_PITCH)^2 <= 1` es una decision tecnica candidata, sin
/// aprobacion visual propia: en diagonal no suma las dos amplitudes.
pub const EYE_FOLLOW_MAX_YAW: f64 = 10.0 * DEGREE;
pub const EYE_FOLLOW_MAX_PITCH: f64 = 8.0 * DEGREE;
/// Rango valido del tiempo de seguimiento (segundos reales hasta el 95 %).
pub const MIN_EYE_FOLLOW_SECONDS: f64 = 0.01;
pub const MAX_EYE_FOLLOW_SECONDS: f64 = 60.0;

/// Ajustes de la mirada. Solo el tiempo es configurable: los limites son constantes del asset
/// (poses aprobadas de V02).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeFollowSettings {
    t95_seconds: f64,
}

impl EyeFollowSettings {
    /// `t95_seconds`: segundos reales para recorrer el 95 % de un cambio de objetivo, en
    /// `[MIN_EYE_FOLLOW_SECONDS, MAX_EYE_FOLLOW_SECONDS]`.
    pub fn new(t95_seconds: f64) -> Result<Self, String> {
        if !(MIN_EYE_FOLLOW_SECONDS..=MAX_EYE_FOLLOW_SECONDS).contains(&t95_seconds) {
            return Err(format!(
                "tiempo de la mirada fuera de [{MIN_EYE_FOLLOW_SECONDS}, {MAX_EYE_FOLLOW_SECONDS}] s: {t95_seconds}"
            ));
        }
        Ok(Self { t95_seconds })
    }
    pub fn t95_seconds(&self) -> f64 {
        self.t95_seconds
    }
}

impl Default for EyeFollowSettings {
    /// Valor tecnico candidato ("acecho"): 1.5 s hasta el 95 %. No es un tiempo probado ni
    /// aprobado visualmente.
    fn default() -> Self {
        Self { t95_seconds: 1.5 }
    }
}

/// Angulos de la mirada (radianes) relativos al eje optico en reposo, siempre finitos y dentro
/// de la elipse de los limites. Convencion de V02: yaw positivo mira a +X y pitch positivo a +Y
/// (espacio de objeto).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GazeAngles {
    yaw: f64,
    pitch: f64,
}

impl GazeAngles {
    pub const NEUTRAL: Self = Self {
        yaw: 0.0,
        pitch: 0.0,
    };
    /// Lleva `(yaw, pitch)` a la elipse escalando hacia el centro (conserva la direccion).
    /// Entradas no finitas, o un escalado que desborde, dan `NEUTRAL`.
    pub fn clamped(yaw: f64, pitch: f64) -> Self {
        if !(yaw.is_finite() && pitch.is_finite()) {
            return Self::NEUTRAL;
        }
        let (u, v) = (yaw / EYE_FOLLOW_MAX_YAW, pitch / EYE_FOLLOW_MAX_PITCH);
        let norm = u.hypot(v);
        if !norm.is_finite() {
            return Self::NEUTRAL;
        }
        if norm <= 1.0 {
            return Self { yaw, pitch };
        }
        Self {
            yaw: u / norm * EYE_FOLLOW_MAX_YAW,
            pitch: v / norm * EYE_FOLLOW_MAX_PITCH,
        }
    }
    pub fn yaw(&self) -> f64 {
        self.yaw
    }
    pub fn pitch(&self) -> f64 {
        self.pitch
    }
    /// Exactamente en reposo (ambos angulos 0).
    pub fn is_neutral(&self) -> bool {
        self.yaw == 0.0 && self.pitch == 0.0
    }
}

/// Desvanecido hacia el reposo segun el angulo entre el eje optico y el objetivo: seguimiento
/// completo hasta `FADE_START`, reposo desde `FADE_END` y `1 - smoothstep` en medio (centrado
/// en 90 grados). Asi no hay salto al cruzar el lateral ni al pasar por detras.
pub const EYE_FOLLOW_FADE_START: f64 = 75.0 * DEGREE;
pub const EYE_FOLLOW_FADE_END: f64 = 105.0 * DEGREE;

/// Angulos de mirada hacia `target_world` (la nave) desde el pivote del ojo en mundo.
///
/// - La direccion se lleva al espacio de objeto con el yaw inverso de la criatura (la escala es
///   uniforme y no cambia angulos); antes se divide por su componente mayor, asi que longitudes
///   enormes o diminutas no desbordan.
/// - El marco del ojo es el eje optico calibrado `a` (no +Z a ciegas), `u = Y x a` y `w = a x u`
///   (con el eje del asset, `u` aprox. +X y `w` aprox. +Y: convencion de V02).
/// - Mapa de angulo y direccion: `theta` = angulo entre `a` y el objetivo; el pedido es
///   `theta` en la direccion `(lx, ly)` del plano tangente, recortado a la elipse y multiplicado
///   por el desvanecido. Es continuo en todas partes (detras el desvanecido ya vale 0).
///
/// Fallback a `NEUTRAL` (nunca NaN): entradas no finitas, objetivo coincidente con el pivote,
/// diferencia que desborda, eje nulo o vertical (sin marco), u objetivo sobre el eje.
pub fn aim_gaze(
    pivot_world: Vec3,
    optical_axis: Vec3,
    creature_yaw: f64,
    target_world: Vec3,
) -> GazeAngles {
    let inputs_finite = pivot_world.is_finite()
        && optical_axis.is_finite()
        && target_world.is_finite()
        && creature_yaw.is_finite();
    if !inputs_finite {
        return GazeAngles::NEUTRAL;
    }
    let delta = target_world - pivot_world;
    let largest = delta.x.abs().max(delta.y.abs()).max(delta.z.abs());
    if !(largest.is_finite() && largest > 0.0) {
        return GazeAngles::NEUTRAL;
    }
    let d = Mat4::rotation_y(-creature_yaw).transform_dir(delta * (1.0 / largest));
    let a = optical_axis.normalized();
    let u = Vec3::Y.cross(a).normalized();
    if a == Vec3::ZERO || u == Vec3::ZERO {
        return GazeAngles::NEUTRAL;
    }
    let w = a.cross(u);
    let (lx, ly, lz) = (d.dot(u), d.dot(w), d.dot(a));
    let tangential = lx.hypot(ly);
    if tangential.is_nan() || tangential <= 1e-12 {
        return GazeAngles::NEUTRAL;
    }
    let theta = tangential.atan2(lz);
    let t = ((theta - EYE_FOLLOW_FADE_START) / (EYE_FOLLOW_FADE_END - EYE_FOLLOW_FADE_START))
        .clamp(0.0, 1.0);
    let fade = 1.0 - t * t * (3.0 - 2.0 * t);
    if fade <= 0.0 {
        return GazeAngles::NEUTRAL;
    }
    let reach = GazeAngles::clamped(theta * lx / tangential, theta * ly / tangential);
    GazeAngles::clamped(reach.yaw * fade, reach.pitch * fade)
}

/// Distancia angular (radianes) por debajo de la cual el estado se fija exactamente en el
/// objetivo: sin esto la persecucion exponencial nunca llegaria al reposo exacto.
pub const EYE_FOLLOW_SNAP_RADIANS: f64 = 1e-6;

/// Estado temporal de la mirada, ligero y separado de la geometria: solo los ajustes y los
/// angulos mostrados. Arranca en reposo; no depende del tiempo absoluto, del O2 ni de las
/// orbitas. **Contrato para la fase 2:** se avanza con segundos **reales** (no el `dt` fisico
/// limitado), F5 no lo congela (F5 solo congela orbitas), y `R`/headless usan `snap`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeFollow {
    settings: EyeFollowSettings,
    current: GazeAngles,
}

impl EyeFollow {
    pub fn new(settings: EyeFollowSettings) -> Self {
        Self {
            settings,
            current: GazeAngles::NEUTRAL,
        }
    }
    pub fn settings(&self) -> EyeFollowSettings {
        self.settings
    }
    pub fn current(&self) -> GazeAngles {
        self.current
    }
    /// Persecucion exponencial hacia `target`: tras `t95_seconds` reales queda el 5 % del
    /// cambio. Es independiente de como se trocee el tiempo (salvo el ajuste final de
    /// `EYE_FOLLOW_SNAP_RADIANS`), no sobrepasa el objetivo y, por convexidad, no sale de la
    /// elipse. `elapsed` cero, negativo o no finito no avanza.
    pub fn update(&mut self, target: GazeAngles, elapsed: f64) {
        if !(elapsed.is_finite() && elapsed > 0.0) {
            return;
        }
        let keep = (-(20.0_f64.ln()) / self.settings.t95_seconds * elapsed).exp();
        let yaw = target.yaw + (self.current.yaw - target.yaw) * keep;
        let pitch = target.pitch + (self.current.pitch - target.pitch) * keep;
        self.current = if (yaw - target.yaw).hypot(pitch - target.pitch) <= EYE_FOLLOW_SNAP_RADIANS
        {
            target
        } else {
            GazeAngles::clamped(yaw, pitch)
        };
    }
    /// Salta al objetivo sin transicion (arranque headless, `R`).
    pub fn snap(&mut self, target: GazeAngles) {
        self.current = target;
    }
    /// Vuelve al reposo exacto.
    pub fn reset(&mut self) {
        self.current = GazeAngles::NEUTRAL;
    }
}

/// Anillo topologico (aristas desde la costura) a partir del cual el ojo es rigido (peso 1).
/// En el asset actual deja 21 vertices, que contienen la pupila (inspeccion de V01/V02).
pub const EYE_RIGID_RING: usize = 3;

/// Calibracion de la mirada local, calculada **una vez** sobre la pose de reposo del modelo
/// (algoritmo de V02 aprobado; especifica del asset, falla cerrada si no calibra):
///
/// - Identidad del ojo: las caras cuyo centroide cae en la `EyeRegion` (el mismo criterio
///   que `StagedParts::split`).
/// - Costura: vertices del ojo que tambien usa alguna cara del cuerpo; peso 0 exacto, igual
///   que todo vertice del cuerpo.
/// - Nucleo rigido: vertices del ojo a `EYE_RIGID_RING` o mas aristas de la costura; peso 1.
/// - Transicion: `smoothstep(d_costura / (d_costura + d_nucleo))` con distancias por
///   longitud de aristas (Dijkstra) dentro de las caras del ojo.
/// - Pivote: centro de la esfera ajustada a los vertices del nucleo (debe quedar detras de la
///   cupula). Eje optico: normal ponderada por area de las caras del ojo cuyos tres vertices
///   no tocan la costura (orientada con las `vn`).
/// - Envolvente posada (`posed_bounds_radius`): radio finito, alrededor de `bounds_center`,
///   que contiene **todos** los vertices del modelo en cualquier pose de la mirada.
///
/// No guarda ni presta textura: se usa la de la `StaticCreature` original.
///
/// **Identidad:** guarda una copia inmutable del modelo calibrado (compartida por sus clones) y
/// solo acepta modelos con el mismo contenido: vertices, UV, normales, caras (indices y
/// material), materiales, bibliotecas y bounds. Otra lectura del mismo OBJ sirve; cualquier
/// edicion, aunque sea en sitio y conserve los tamanos, no. Dos skins son el mismo si sus
/// valores coinciden (un clon o una recalibracion identica).
#[derive(Clone, Debug, PartialEq)]
pub struct EyeSkin {
    bind: BindSnapshot,
    eye_faces: Vec<bool>,
    weights: Vec<f64>,
    moving: Vec<usize>,
    shared_vertices: usize,
    core_vertices: usize,
    pivot: Vec3,
    optical_axis: Vec3,
    posed_bounds_radius: f64,
    bind_bounds_radius: f64,
}

impl EyeSkin {
    /// Calibra sobre la pose de reposo. `Err` (sin panico) con vertices no finitos, normales
    /// no finitas o de longitud no representable, caras fuera de rango, region vacia o total,
    /// ojo sin costura, vertices inalcanzables, nucleo con menos de 4 vertices o sin esfera
    /// unica (pivote), eje nulo, pivote delante de la cupula, bounds no finitos o con radio
    /// negativo, o envolvente posada no finita.
    pub fn calibrate(model: &Model, region: &EyeRegion) -> Result<Self, String> {
        let n = model.vertices.len();
        if model.vertices.iter().flatten().any(|c| !c.is_finite()) {
            return Err("vertices no finitos en el modelo".into());
        }
        // Girar conserva la longitud: si la de cada normal es representable, ninguna normal
        // posada desborda. No se normalizan (la pose de V02 parte de las originales).
        if model
            .normals
            .iter()
            .any(|&nn| !Vec3::from_array(nn).length().is_finite())
        {
            return Err("normales no finitas o de longitud no representable en el modelo".into());
        }
        if let Some(f) = model
            .faces
            .iter()
            .find(|f| f.vertex_indices.iter().any(|&i| i >= n))
        {
            return Err(format!("cara fuera de rango: {:?}", f.vertex_indices));
        }
        let v = |i: usize| Vec3::from_array(model.vertices[i]);
        let eye_faces: Vec<bool> = model
            .faces
            .iter()
            .map(|f| {
                let [a, b, c] = f.vertex_indices.map(v);
                ((a + b + c) * (1.0 / 3.0) - region.center()).length() < region.radius()
            })
            .collect();
        match eye_faces.iter().filter(|&&e| e).count() {
            0 => return Err("la region del ojo no contiene ninguna cara".into()),
            count if count == model.faces.len() => {
                return Err("la region del ojo contiene todas las caras".into());
            }
            _ => {}
        }
        let (mut eye_vertex, mut body_vertex) = (vec![false; n], vec![false; n]);
        for (f, &eye) in model.faces.iter().zip(&eye_faces) {
            for &i in &f.vertex_indices {
                if eye {
                    eye_vertex[i] = true;
                } else {
                    body_vertex[i] = true;
                }
            }
        }
        let shared: Vec<usize> = (0..n)
            .filter(|&i| eye_vertex[i] && body_vertex[i])
            .collect();
        if shared.is_empty() {
            return Err("el ojo no tiene costura con el cuerpo".into());
        }
        let eye_tris: Vec<[usize; 3]> = model
            .faces
            .iter()
            .zip(&eye_faces)
            .filter(|(_, e)| **e)
            .map(|(f, _)| f.vertex_indices)
            .collect();
        let ring = ring_distances(&eye_tris, &shared, n);
        if (0..n).any(|i| eye_vertex[i] && ring[i].is_none()) {
            return Err("hay vertices del ojo inalcanzables desde la costura".into());
        }
        let core: Vec<usize> = (0..n)
            .filter(|&i| eye_vertex[i] && ring[i].is_some_and(|r| r >= EYE_RIGID_RING))
            .collect();
        if core.len() < 4 {
            return Err(format!(
                "nucleo rigido insuficiente (anillo >= {EYE_RIGID_RING}): {} vertices",
                core.len()
            ));
        }
        let d_border = edge_distances(&model.vertices, &eye_tris, &shared);
        let d_core = edge_distances(&model.vertices, &eye_tris, &core);
        let mut weights = vec![0.0; n];
        for i in (0..n).filter(|&i| eye_vertex[i]) {
            let (Some(b), Some(c)) = (d_border[i], d_core[i]) else {
                return Err(format!("vertice del ojo {i} inalcanzable"));
            };
            // Costura exacta (b = 0) y nucleo exacto (c = 0); no pueden coincidir porque el
            // nucleo esta a 3 o mas aristas de la costura.
            weights[i] = if b == 0.0 {
                0.0
            } else if c == 0.0 {
                1.0
            } else {
                let t = b / (b + c);
                t * t * (3.0 - 2.0 * t)
            };
        }
        let moving: Vec<usize> = (0..n).filter(|&i| weights[i] > 0.0).collect();
        let pivot = sphere_center(&core.iter().map(|&i| v(i)).collect::<Vec<_>>())
            .map_err(|e| format!("no se puede ajustar el pivote del nucleo: {e}"))?;
        let mut axis = Vec3::ZERO;
        for (f, _) in model.faces.iter().zip(&eye_faces).filter(|(_, e)| **e) {
            if f.vertex_indices.iter().any(|&i| ring[i] == Some(0)) {
                continue;
            }
            let [a, b, c] = f.vertex_indices.map(v);
            let normal = (b - a).cross(c - a);
            let reference = match f.normal_indices {
                Some(ni) => ni.iter().fold(Vec3::ZERO, |acc, &k| {
                    acc + model
                        .normals
                        .get(k)
                        .map_or(Vec3::ZERO, |&nn| Vec3::from_array(nn))
                }),
                None => (a + b + c) * (1.0 / 3.0) - pivot,
            };
            axis += if normal.dot(reference) < 0.0 {
                -normal
            } else {
                normal
            };
        }
        let optical_axis = axis.normalized();
        if optical_axis == Vec3::ZERO {
            return Err("eje optico nulo".into());
        }
        let core_centroid =
            core.iter().fold(Vec3::ZERO, |acc, &i| acc + v(i)) * (1.0 / core.len() as f64);
        if (core_centroid - pivot).dot(optical_axis) <= 0.0 {
            return Err("el pivote no queda detras de la cupula del ojo".into());
        }
        let center = Vec3::from_array(model.bounds_center);
        if !(center.is_finite() && model.bounds_radius.is_finite() && model.bounds_radius >= 0.0) {
            return Err(format!(
                "bounds del modelo sin integridad: centro {center:?}, radio {}",
                model.bounds_radius
            ));
        }
        // Envolvente de toda la geometria en cualquier pose, sin fiarse de la esfera declarada:
        // los vertices quietos tal cual y los que se mueven girando alrededor del pivote (el
        // giro conserva |v - pivote|). Con bounds de `update_bounds` (misma formula de
        // distancia) los quietos no la agrandan y queda la cota de los que se mueven.
        let mut posed_bounds_radius = model.bounds_radius;
        for (i, &w) in weights.iter().enumerate() {
            let reach = if w > 0.0 {
                (pivot - center).length() + (v(i) - pivot).length()
            } else {
                (v(i) - center).length()
            };
            if !reach.is_finite() {
                return Err("la envolvente posada de los bounds no es finita".into());
            }
            posed_bounds_radius = posed_bounds_radius.max(reach);
        }
        Ok(Self {
            bind: BindSnapshot(Arc::new(copy_model(model))),
            eye_faces,
            weights,
            moving,
            shared_vertices: shared.len(),
            core_vertices: core.len(),
            pivot,
            optical_axis,
            posed_bounds_radius,
            bind_bounds_radius: model.bounds_radius,
        })
    }
    pub fn eye_faces(&self) -> &[bool] {
        &self.eye_faces
    }
    pub fn weights(&self) -> &[f64] {
        &self.weights
    }
    pub fn moving_vertices(&self) -> &[usize] {
        &self.moving
    }
    pub fn shared_vertices(&self) -> usize {
        self.shared_vertices
    }
    pub fn core_vertices(&self) -> usize {
        self.core_vertices
    }
    pub fn pivot(&self) -> Vec3 {
        self.pivot
    }
    pub fn optical_axis(&self) -> Vec3 {
        self.optical_axis
    }
    pub fn posed_bounds_radius(&self) -> f64 {
        self.posed_bounds_radius
    }
    pub fn fits_bind_sphere(&self) -> bool {
        self.posed_bounds_radius <= self.bind_bounds_radius
    }
    /// `Err` si `model` no tiene el contenido del modelo calibrado. Recorre el modelo una vez
    /// sin reservar memoria.
    fn check_model(&self, model: &Model) -> Result<(), String> {
        if !same_model(&self.bind.0, model) {
            return Err("la calibracion del ojo es de otro modelo".into());
        }
        Ok(())
    }
    /// Angulos hacia `target_world` (la nave) desde el pivote de esta criatura en el mundo.
    pub fn aim(&self, creature: &StaticCreature, target_world: Vec3) -> GazeAngles {
        let pivot_world = creature.transform().matrix().transform_point(self.pivot);
        aim_gaze(
            pivot_world,
            self.optical_axis,
            creature.placement().yaw(),
            target_world,
        )
    }
}

/// Geometria posada del ojo, construida **una vez** y actualizada en sitio en cada pose.
///
/// Contiene solo las caras del ojo (en el orden original), con una copia de los vertices del
/// modelo (las caras indexan el arreglo global; asi conserva bounds y transform exactos), los
/// UV y materiales originales, y **copias privadas por esquina** de sus normales (las
/// originales, que comparte el cuerpo, nunca cambian). El cuerpo se dibuja con su parte de
/// reposo (`StagedParts::body`): no hace falta un modelo completo posado.
///
/// Cada `pose` parte del modelo de reposo (no acumula), escribe solo los vertices con peso y
/// las normales copiadas, sin reservar memoria. La normal posada gira con el peso de su
/// vertice: **aproximacion declarada**, no el jacobiano exacto de la deformacion.
///
/// Guarda un clon del skin con que se construyo: `pose` exige ese skin (por valor) y su modelo
/// (por contenido, ver `EyeSkin`). Coste por cuadro sin reservas: comparar el skin (pesos y
/// caras; la copia del modelo compartida se reconoce por puntero) y recorrer el modelo una vez.
pub struct EyePoseCache {
    skin: EyeSkin,
    eye_model: Model,
    /// `(copia, normal original, vertice)` de cada esquina de cara del ojo.
    corner_normals: Vec<(usize, usize, usize)>,
    posed: GazeAngles,
}

impl EyePoseCache {
    /// `Err` si `skin` no se calibro para `bind` (por contenido) o si una cara del ojo usa
    /// normales fuera de rango.
    pub fn new(bind: &Model, skin: &EyeSkin) -> Result<Self, String> {
        skin.check_model(bind)?;
        let mut normals = bind.normals.clone();
        let mut corner_normals = Vec::new();
        let mut faces = Vec::new();
        for (face, _) in bind.faces.iter().zip(&skin.eye_faces).filter(|(_, e)| **e) {
            let normal_indices = match face.normal_indices {
                Some(ni) => {
                    if ni.iter().any(|&k| k >= bind.normals.len()) {
                        return Err(format!("normal fuera de rango en la cara {ni:?}"));
                    }
                    Some(std::array::from_fn(|slot| {
                        normals.push(bind.normals[ni[slot]]);
                        let copy = normals.len() - 1;
                        corner_normals.push((copy, ni[slot], face.vertex_indices[slot]));
                        copy
                    }))
                }
                None => None,
            };
            faces.push(Face {
                vertex_indices: face.vertex_indices,
                uv_indices: face.uv_indices,
                normal_indices,
                material: face.material.clone(),
            });
        }
        Ok(Self {
            skin: skin.clone(),
            eye_model: Model {
                vertices: bind.vertices.clone(),
                uvs: bind.uvs.clone(),
                normals,
                faces,
                material_libraries: bind.material_libraries.clone(),
                materials: bind.materials.clone(),
                bounds_center: bind.bounds_center,
                bounds_radius: bind.bounds_radius,
            },
            corner_normals,
            posed: GazeAngles::NEUTRAL,
        })
    }

    /// Pone el ojo en `angles` desde la pose de reposo `bind`. Todo se valida antes de
    /// escribir: con `Err` (otro modelo u otro skin que los de la construccion, o algun
    /// resultado no finito) el cache queda como estaba. En `NEUTRAL` copia los valores de
    /// reposo exactos.
    pub fn pose(&mut self, bind: &Model, skin: &EyeSkin, angles: GazeAngles) -> Result<(), String> {
        if *skin != self.skin {
            return Err("el cache se construyo con otra calibracion del ojo".into());
        }
        skin.check_model(bind)?;
        let (yaw, pitch) = (angles.yaw, angles.pitch);
        let neutral = angles.is_neutral();
        let vertex = |i: usize| {
            let w = skin.weights[i];
            if neutral {
                bind.vertices[i]
            } else {
                let p = Vec3::from_array(bind.vertices[i]);
                array(skin.pivot + rotate_look(p - skin.pivot, w * yaw, w * pitch))
            }
        };
        let normal = |source: usize, v: usize| {
            let w = skin.weights[v];
            let n = Vec3::from_array(bind.normals[source]);
            array(rotate_look(n, w * yaw, w * pitch))
        };
        // Primera pasada sin escribir: todas las salidas deben ser finitas. La segunda repite
        // el mismo calculo (determinista) y escribe; ninguna reserva memoria.
        let finite = |q: [f64; 3]| q.iter().all(|c| c.is_finite());
        let representable = skin.moving.iter().all(|&i| finite(vertex(i)))
            && self
                .corner_normals
                .iter()
                .all(|&(_, source, v)| finite(normal(source, v)));
        if !representable {
            return Err("la pose del ojo produce valores no finitos".into());
        }
        for &i in &skin.moving {
            self.eye_model.vertices[i] = vertex(i);
        }
        for &(copy, source, v) in &self.corner_normals {
            self.eye_model.normals[copy] = normal(source, v);
        }
        self.posed = angles;
        Ok(())
    }
    pub fn eye_model(&self) -> &Model {
        &self.eye_model
    }
    pub fn posed(&self) -> GazeAngles {
        self.posed
    }
}

/// Giro de mirada de V02: primero `pitch` (positivo lleva +Z hacia +Y) y despues `yaw`
/// alrededor de +Y (positivo lleva +Z hacia +X). Con ambos en 0 devuelve `v` exacto.
fn rotate_look(v: Vec3, yaw: f64, pitch: f64) -> Vec3 {
    if yaw == 0.0 && pitch == 0.0 {
        return v;
    }
    let (sp, cp) = pitch.sin_cos();
    let p = Vec3::new(v.x, v.y * cp + v.z * sp, -v.y * sp + v.z * cp);
    let (sy, cy) = yaw.sin_cos();
    Vec3::new(p.x * cy + p.z * sy, p.y, -p.x * sy + p.z * cy)
}

fn array(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

/// Copia inmutable del modelo de reposo de una calibracion. Sus clones comparten la misma
/// copia: se reconocen por puntero y, si no, se comparan por contenido.
#[derive(Clone)]
struct BindSnapshot(Arc<Model>);

impl std::fmt::Debug for BindSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BindSnapshot({} vertices, {} caras)",
            self.0.vertices.len(),
            self.0.faces.len()
        )
    }
}

impl PartialEq for BindSnapshot {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || same_model(&self.0, &other.0)
    }
}

fn copy_model(model: &Model) -> Model {
    Model {
        vertices: model.vertices.clone(),
        uvs: model.uvs.clone(),
        normals: model.normals.clone(),
        faces: model
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: model.material_libraries.clone(),
        materials: model.materials.clone(),
        bounds_center: model.bounds_center,
        bounds_radius: model.bounds_radius,
    }
}

/// Mismo contenido en todos los campos (`==` de f64: un NaN nunca coincide). Sin reservas.
fn same_model(a: &Model, b: &Model) -> bool {
    a.vertices == b.vertices
        && a.uvs == b.uvs
        && a.normals == b.normals
        && a.faces.len() == b.faces.len()
        && a.faces.iter().zip(&b.faces).all(|(f, g)| {
            f.vertex_indices == g.vertex_indices
                && f.uv_indices == g.uv_indices
                && f.normal_indices == g.normal_indices
                && f.material == g.material
        })
        && a.material_libraries == b.material_libraries
        && a.materials == b.materials
        && a.bounds_center == b.bounds_center
        && a.bounds_radius == b.bounds_radius
}

/// Distancia en aristas (BFS) desde `seeds` dentro de `faces`; `None` si no se alcanza.
fn ring_distances(faces: &[[usize; 3]], seeds: &[usize], n: usize) -> Vec<Option<usize>> {
    let mut adjacency = vec![Vec::new(); n];
    for f in faces {
        for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
            adjacency[a].push(b);
            adjacency[b].push(a);
        }
    }
    let mut ring = vec![None; n];
    let mut queue = std::collections::VecDeque::new();
    for &s in seeds {
        ring[s] = Some(0);
        queue.push_back(s);
    }
    while let Some(i) = queue.pop_front() {
        let next = ring[i].map(|r| r + 1);
        for &j in &adjacency[i] {
            if ring[j].is_none() {
                ring[j] = next;
                queue.push_back(j);
            }
        }
    }
    ring
}

/// Distancia mas corta por longitud de aristas (Dijkstra) desde `seeds` dentro de `faces`.
fn edge_distances(
    positions: &[[f64; 3]],
    faces: &[[usize; 3]],
    seeds: &[usize],
) -> Vec<Option<f64>> {
    let n = positions.len();
    let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for f in faces {
        for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
            let length = (Vec3::from_array(positions[b]) - Vec3::from_array(positions[a])).length();
            adjacency[a].push((b, length));
            adjacency[b].push((a, length));
        }
    }
    #[derive(PartialEq)]
    struct Item(f64, usize);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Item {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other.0.total_cmp(&self.0) // monticulo de minimos
        }
    }
    let mut distance = vec![None; n];
    let mut heap = std::collections::BinaryHeap::new();
    for &s in seeds {
        distance[s] = Some(0.0);
        heap.push(Item(0.0, s));
    }
    while let Some(Item(d, i)) = heap.pop() {
        if distance[i].is_some_and(|best| d > best) {
            continue;
        }
        for &(j, length) in &adjacency[i] {
            let candidate = d + length;
            if distance[j].is_none_or(|best| candidate < best) {
                distance[j] = Some(candidate);
                heap.push(Item(candidate, j));
            }
        }
    }
    distance
}

/// Centro de la esfera por minimos cuadrados algebraicos (`x^2+y^2+z^2 + Dx+Ey+Fz+G = 0`).
/// `Err` con menos de 4 puntos o sistema singular (puntos coplanares).
fn sphere_center(points: &[Vec3]) -> Result<Vec3, String> {
    if points.len() < 4 {
        return Err(format!("{} puntos", points.len()));
    }
    let mut m = [[0.0f64; 5]; 4];
    for p in points {
        let row = [p.x, p.y, p.z, 1.0];
        let b = -p.dot(*p);
        for (i, mi) in m.iter_mut().enumerate() {
            for (j, mij) in mi.iter_mut().take(4).enumerate() {
                *mij += row[i] * row[j];
            }
            mi[4] += row[i] * b;
        }
    }
    let scale = m
        .iter()
        .flat_map(|r| r[..4].iter())
        .fold(0.0f64, |a, x| a.max(x.abs()));
    for col in 0..4 {
        let pivot = (col..4)
            .max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))
            .unwrap_or(col);
        let pivot_value = m[pivot][col].abs();
        if pivot_value.is_nan() || pivot_value <= 1e-12 * scale {
            return Err("puntos coplanares: sin esfera unica".into());
        }
        m.swap(col, pivot);
        let pivot_row = m[col];
        for (row, values) in m.iter_mut().enumerate() {
            if row != col {
                let f = values[col] / pivot_row[col];
                for (value, &p) in values.iter_mut().zip(&pivot_row).skip(col) {
                    *value -= f * p;
                }
            }
        }
    }
    let center = Vec3::new(
        -m[0][4] / m[0][0] / 2.0,
        -m[1][4] / m[1][1] / 2.0,
        -m[2][4] / m[2][2] / 2.0,
    );
    if !center.is_finite() {
        return Err("centro no finito".into());
    }
    Ok(center)
}
