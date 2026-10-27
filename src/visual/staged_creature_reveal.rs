//! Revelacion de la criatura por etapas (debug, opt-in): primero el ojo, despues el cuerpo.
//! Ver `docs/static-creature.md`.

use crate::{
    assets::obj::{Face, Model},
    math::Vec3,
    visual::creature_reveal::{RevealSettings, step_reveal},
};

/// Combinaciones de `--creature-reveal-stages`, sin tocar archivos. Se valida despues de
/// `CreatureOptions::from_flags` y `validate_reveal_flags`.
pub fn validate_staged_flags(
    stages: bool,
    has_creature: bool,
    uniform_reveal: bool,
    legacy: bool,
) -> Result<(), String> {
    if !stages {
        return Ok(());
    }
    if legacy {
        return Err("--legacy no admite --creature-reveal-stages".into());
    }
    if uniform_reveal {
        return Err(
            "--creature-reveal y --creature-reveal-stages son incompatibles: elige un modo".into(),
        );
    }
    if !has_creature {
        return Err(
            "--creature-reveal-stages necesita --creature-model y --creature-texture".into(),
        );
    }
    Ok(())
}

/// Region del ojo en espacio de objeto del OBJ (antes del transform): una cara es del ojo si
/// su centroide queda a menos de `radius` de `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeRegion {
    center: Vec3,
    radius: f64,
}

/// ROI frontal **calibrada para el asset Meshy actual** (variante B, "ojo con un poco de
/// cuenca"): centro medido desde la camara `--near-creature`, radio 0.20; en ese asset da 291
/// caras. No es una identidad generica: otra malla o un remesh exigen recalibrar, y una
/// region vacia o total se rechaza al cargar (`StagedParts::split`).
pub const CREATURE_EYE_REGION: EyeRegion = EyeRegion {
    center: Vec3::new(0.049, 0.059, 0.311),
    radius: 0.20,
};

impl EyeRegion {
    /// Centro finito y radio finito mayor que 0.
    pub fn new(center: Vec3, radius: f64) -> Result<Self, String> {
        if !center.is_finite() {
            return Err(format!("centro de la region del ojo no finito: {center:?}"));
        }
        if !(radius.is_finite() && radius > 0.0) {
            return Err(format!("radio de la region del ojo invalido: {radius}"));
        }
        Ok(Self { center, radius })
    }
    pub fn center(&self) -> Vec3 {
        self.center
    }
    pub fn radius(&self) -> f64 {
        self.radius
    }
    fn contains(&self, model: &Model, face: &Face) -> bool {
        let [a, b, c] = face
            .vertex_indices
            .map(|i| Vec3::from_array(model.vertices[i]));
        ((a + b + c) * (1.0 / 3.0) - self.center).length() < self.radius
    }
}

/// Cache de la particion en ojo y cuerpo, calculada **una vez** por modelo (no por cuadro).
/// Las dos partes copian vertices, UV, normales, materiales y bounds del modelo original, asi
/// que dan exactamente su transform; las caras se reparten sin solaparse y sin perder
/// ninguna. No guarda textura: se usa la de la `StaticCreature` original.
pub struct StagedParts {
    eye: Model,
    body: Model,
}

impl StagedParts {
    /// Falla (cerrado) si la region no toma ninguna cara del ojo o las toma todas.
    pub fn split(model: &Model, region: &EyeRegion) -> Result<Self, String> {
        let is_eye: Vec<bool> = model
            .faces
            .iter()
            .map(|face| region.contains(model, face))
            .collect();
        let eye_faces = is_eye.iter().filter(|&&e| e).count();
        if eye_faces == 0 {
            return Err(format!(
                "la region del ojo {region:?} no contiene ninguna cara: recalibrar para este modelo"
            ));
        }
        if eye_faces == model.faces.len() {
            return Err(format!(
                "la region del ojo {region:?} contiene todas las caras: no queda cuerpo"
            ));
        }
        let part = |want: bool| Model {
            vertices: model.vertices.clone(),
            uvs: model.uvs.clone(),
            normals: model.normals.clone(),
            faces: model
                .faces
                .iter()
                .zip(&is_eye)
                .filter(|(_, e)| **e == want)
                .map(|(f, _)| Face {
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
        };
        Ok(Self {
            eye: part(true),
            body: part(false),
        })
    }
    pub fn eye(&self) -> &Model {
        &self.eye
    }
    pub fn body(&self) -> &Model {
        &self.body
    }
    pub fn eye_faces(&self) -> usize {
        self.eye.faces.len()
    }
    pub fn body_faces(&self) -> usize {
        self.body.faces.len()
    }
    /// Memoria aproximada de la cache (arreglos duplicados y caras), en bytes.
    pub fn cache_bytes(&self) -> usize {
        [&self.eye, &self.body]
            .iter()
            .map(|m| {
                m.vertices.len() * size_of::<[f64; 3]>()
                    + m.uvs.len() * size_of::<[f64; 2]>()
                    + m.normals.len() * size_of::<[f64; 3]>()
                    + m.faces.len() * size_of::<Face>()
            })
            .sum()
    }
}

/// Ajustes de la revelacion por etapas. Solo se construyen validados.
///
/// Un unico progreso normalizado `p` en `[0, 1]` sigue al O2 de forma lineal por tramos:
/// - de `start_oxygen` a `eye_full_oxygen`, el ojo pasa de 0 a 1 (smoothstep);
/// - de `eye_full_oxygen` a `hint_oxygen`, el cuerpo pasa de 0 a `hint_alpha` (smoothstep),
///   el "contorno muy tenue";
/// - de `hint_oxygen` a `full_oxygen`, el cuerpo pasa de `hint_alpha` a 1 (smoothstep).
///
/// Los alfas son **opacidad lineal** del compositor, no brillo percibido.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StagedRevealSettings {
    start_oxygen: f64,
    eye_full_oxygen: f64,
    hint_oxygen: f64,
    hint_alpha: f64,
    full_oxygen: f64,
    /// Tiempos de transicion (los de V1) para `step_reveal` sobre el progreso.
    transition: RevealSettings,
}

impl StagedRevealSettings {
    /// `0 <= full < hint < eye_full < start <= 1`, `hint_alpha` en `[0, 1)` y tiempos como
    /// `RevealSettings::new` (segundos hasta el 95 %, en `[0.01, 60]`).
    pub fn new(
        start_oxygen: f64,
        eye_full_oxygen: f64,
        hint_oxygen: f64,
        hint_alpha: f64,
        full_oxygen: f64,
        rise_seconds: f64,
        fall_seconds: f64,
    ) -> Result<Self, String> {
        let ordered = 0.0 <= full_oxygen
            && full_oxygen < hint_oxygen
            && hint_oxygen < eye_full_oxygen
            && eye_full_oxygen < start_oxygen
            && start_oxygen <= 1.0;
        if !ordered {
            return Err(format!(
                "umbrales de etapas invalidos: se necesita 0 <= {full_oxygen} < {hint_oxygen} < {eye_full_oxygen} < {start_oxygen} <= 1"
            ));
        }
        let span = start_oxygen - full_oxygen;
        let eye_point = (start_oxygen - eye_full_oxygen) / span;
        let hint_point = (start_oxygen - hint_oxygen) / span;
        if !(eye_point.is_finite()
            && hint_point.is_finite()
            && 0.0 < eye_point
            && eye_point < hint_point
            && hint_point < 1.0)
        {
            return Err(format!(
                "umbrales derivados de etapas invalidos: se necesita 0 < {eye_point} < {hint_point} < 1, finitos y distintos"
            ));
        }
        if !(0.0..1.0).contains(&hint_alpha) {
            return Err(format!("alfa del contorno fuera de [0, 1): {hint_alpha}"));
        }
        Ok(Self {
            start_oxygen,
            eye_full_oxygen,
            hint_oxygen,
            hint_alpha,
            full_oxygen,
            transition: RevealSettings::new(start_oxygen, full_oxygen, rise_seconds, fall_seconds)?,
        })
    }
    pub fn start_oxygen(&self) -> f64 {
        self.start_oxygen
    }
    pub fn eye_full_oxygen(&self) -> f64 {
        self.eye_full_oxygen
    }
    pub fn hint_oxygen(&self) -> f64 {
        self.hint_oxygen
    }
    pub fn hint_alpha(&self) -> f64 {
        self.hint_alpha
    }
    pub fn full_oxygen(&self) -> f64 {
        self.full_oxygen
    }
    pub fn rise_seconds(&self) -> f64 {
        self.transition.rise_seconds()
    }
    pub fn fall_seconds(&self) -> f64 {
        self.transition.fall_seconds()
    }
    /// Progreso en el que el ojo queda completo y en el que el cuerpo llega a `hint_alpha`.
    fn breakpoints(&self) -> (f64, f64) {
        let span = self.start_oxygen - self.full_oxygen;
        (
            (self.start_oxygen - self.eye_full_oxygen) / span,
            (self.start_oxygen - self.hint_oxygen) / span,
        )
    }
}

impl Default for StagedRevealSettings {
    /// Candidatas (no aprobadas): ojo de 50 % a 30 %, contorno 2 % al 25 %, entera al 5 %;
    /// aparece en ~2 s y se oculta en ~0.7 s (95 %), como V1.
    fn default() -> Self {
        Self::new(0.50, 0.30, 0.25, 0.02, 0.05, 2.0, 0.7).expect("ajustes por defecto validos")
    }
}

/// Progreso objetivo en `[0, 1]` para un oxigeno: lineal por tramos y monotono, con
/// umbrales inclusivos para que los pasos de depuracion exactos (0.30, 0.25, 0.05) caigan en
/// los puntos de quiebre. Fuera de `[0, 1]` se recorta; no finito oculta (0).
pub fn staged_target(oxygen01: f64, settings: &StagedRevealSettings) -> f64 {
    if !oxygen01.is_finite() {
        return 0.0;
    }
    let s = settings;
    let o2 = oxygen01.clamp(0.0, 1.0);
    let (eye_point, hint_point) = s.breakpoints();
    let progress = if o2 <= s.full_oxygen {
        1.0
    } else if o2 <= s.hint_oxygen {
        hint_point + (1.0 - hint_point) * (s.hint_oxygen - o2) / (s.hint_oxygen - s.full_oxygen)
    } else if o2 <= s.eye_full_oxygen {
        eye_point
            + (hint_point - eye_point) * (s.eye_full_oxygen - o2)
                / (s.eye_full_oxygen - s.hint_oxygen)
    } else {
        eye_point * ((s.start_oxygen - o2) / (s.start_oxygen - s.eye_full_oxygen)).clamp(0.0, 1.0)
    };
    progress.clamp(0.0, 1.0)
}

/// `(alfa_ojo, alfa_cuerpo)` en `[0, 1]` derivados de un unico progreso: el cuerpo solo es
/// positivo cuando el ojo vale exactamente 1, tambien durante la transicion. Progreso NaN se
/// trata como 0; fuera de `[0, 1]` se recorta.
pub fn staged_alphas(progress: f64, settings: &StagedRevealSettings) -> (f64, f64) {
    let p = if progress.is_nan() {
        0.0
    } else {
        progress.clamp(0.0, 1.0)
    };
    if p >= 1.0 {
        return (1.0, 1.0);
    }
    let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
    let (eye_point, hint_point) = settings.breakpoints();
    if p < eye_point {
        return (smooth(p / eye_point), 0.0);
    }
    let hint = settings.hint_alpha;
    let body = if p <= hint_point {
        hint * smooth((p - eye_point) / (hint_point - eye_point))
    } else {
        hint + (1.0 - hint) * smooth((p - hint_point) / (1.0 - hint_point))
    };
    (1.0, body.clamp(0.0, 1.0))
}

/// Estado del efecto por etapas: los ajustes y el progreso mostrado en `[0, 1]`. Como V1,
/// no guarda tiempo absoluto y avanza con `step_reveal` y segundos reales.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StagedReveal {
    settings: StagedRevealSettings,
    shown: f64,
}

impl StagedReveal {
    /// Arranca ya en el objetivo de `oxygen01`, sin transicion.
    pub fn new(settings: StagedRevealSettings, oxygen01: f64) -> Self {
        Self {
            settings,
            shown: staged_target(oxygen01, &settings),
        }
    }
    pub fn settings(&self) -> StagedRevealSettings {
        self.settings
    }
    /// Progreso mostrado en `[0, 1]`.
    pub fn shown(&self) -> f64 {
        self.shown
    }
    pub fn alphas(&self) -> (f64, f64) {
        staged_alphas(self.shown, &self.settings)
    }
    pub fn target(&self, oxygen01: f64) -> f64 {
        staged_target(oxygen01, &self.settings)
    }
    /// Avanza `elapsed` segundos reales hacia el objetivo de `oxygen01` (`step_reveal`).
    pub fn update(&mut self, oxygen01: f64, elapsed: f64) {
        self.shown = step_reveal(
            self.shown,
            self.target(oxygen01),
            elapsed,
            &self.settings.transition,
        );
    }
    /// Salta al objetivo de `oxygen01` sin transicion.
    pub fn snap(&mut self, oxygen01: f64) {
        self.shown = self.target(oxygen01);
    }
}
