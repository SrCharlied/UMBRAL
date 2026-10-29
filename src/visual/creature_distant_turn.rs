//! Giro global lejano (candidata experimental, opt-in): de lejos la criatura entera gira, lenta
//! y limitada, hacia el punto que mira el ojo; de cerca se queda quieta y solo mira el ojo. Es
//! una rotacion rigida de runtime alrededor del centro envolvente fijo de `CreaturePlacement`:
//! no mueve el centro ni copia geometria. Aqui vive el nucleo puro. Ver
//! `docs/static-creature.md`.

use crate::{
    debug::overlay::draw_text,
    math::{Mat4, Transform, Vec3},
    renderer::framebuffer::Framebuffer,
    visual::{
        creature_eye_comparison::PanelLayout,
        creature_eye_follow::{EYE_FOLLOW_FADE_END, EYE_FOLLOW_FADE_START},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

/// Combinaciones de `--creature-distant-turn`, sin tocar archivos. Se valida despues de
/// `validate_eye_compare_flags`: sus errores (y los anteriores) se conservan.
pub fn validate_distant_turn_flags(
    distant_turn: bool,
    compare: bool,
    legacy: bool,
) -> Result<(), String> {
    if !distant_turn {
        return Ok(());
    }
    if legacy {
        return Err("--legacy no admite --creature-distant-turn".into());
    }
    if !compare {
        return Err("--creature-distant-turn necesita --creature-eye-compare".into());
    }
    Ok(())
}

const DEGREE: f64 = std::f64::consts::PI / 180.0;
/// Limites del giro global (radianes), envolvente eliptica como la del ojo. Valores tecnicos
/// candidatos, sin aprobacion artistica.
pub const DISTANT_TURN_MAX_YAW: f64 = 20.0 * DEGREE;
pub const DISTANT_TURN_MAX_PITCH: f64 = 12.0 * DEGREE;
/// Segundos reales hasta el 95 % de un cambio de objetivo (el ojo usa 1.5 s).
pub const DISTANT_TURN_T95_SECONDS: f64 = 8.0;
/// Distancia de la camara al centro, en radios, con peso 0 (cerca) y peso 1 (lejos).
pub const DISTANT_TURN_NEAR_RADII: f64 = 3.0;
pub const DISTANT_TURN_FAR_RADII: f64 = 7.0;

/// Peso del giro global segun la distancia camara-centro en radios: 0 hasta
/// `DISTANT_TURN_NEAR_RADII`, 1 desde `DISTANT_TURN_FAR_RADII` y smoothstep en medio (sin
/// salto ni derivada rota en los umbrales). Una distancia no finita o negativa da 0: sin dato
/// valido no hay giro.
pub fn distance_weight(distance_radii: f64) -> f64 {
    if !(distance_radii.is_finite() && distance_radii >= 0.0) {
        return 0.0;
    }
    let t = ((distance_radii - DISTANT_TURN_NEAR_RADII)
        / (DISTANT_TURN_FAR_RADII - DISTANT_TURN_NEAR_RADII))
        .clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Angulos del giro global (radianes) sobre el marco de reposo de la criatura, siempre finitos
/// y dentro de la elipse 20/12. Convencion: yaw positivo lleva el frente (+Z del OBJ) hacia +X
/// del OBJ; pitch positivo lo lleva hacia arriba (+Y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnAngles {
    yaw: f64,
    pitch: f64,
}

impl TurnAngles {
    pub const NEUTRAL: Self = Self {
        yaw: 0.0,
        pitch: 0.0,
    };
    /// Lleva `(yaw, pitch)` a la elipse escalando hacia el centro; no finitos dan `NEUTRAL`.
    pub fn clamped(yaw: f64, pitch: f64) -> Self {
        if !(yaw.is_finite() && pitch.is_finite()) {
            return Self::NEUTRAL;
        }
        let (u, v) = (yaw / DISTANT_TURN_MAX_YAW, pitch / DISTANT_TURN_MAX_PITCH);
        let norm = u.hypot(v);
        if !norm.is_finite() {
            return Self::NEUTRAL;
        }
        if norm <= 1.0 {
            return Self { yaw, pitch };
        }
        Self {
            yaw: u / norm * DISTANT_TURN_MAX_YAW,
            pitch: v / norm * DISTANT_TURN_MAX_PITCH,
        }
    }
    pub fn yaw(&self) -> f64 {
        self.yaw
    }
    pub fn pitch(&self) -> f64 {
        self.pitch
    }
    pub fn is_neutral(&self) -> bool {
        self.yaw == 0.0 && self.pitch == 0.0
    }
}

/// Direccion del centro a `target` en el marco de reposo (yaw inverso de la criatura), dividida
/// antes por su componente mayor para no desbordar. `None` sin direccion o con no finitos.
fn rest_direction(center: Vec3, creature_yaw: f64, target: Vec3) -> Option<Vec3> {
    if !(center.is_finite() && creature_yaw.is_finite() && target.is_finite()) {
        return None;
    }
    let delta = target - center;
    let largest = delta.x.abs().max(delta.y.abs()).max(delta.z.abs());
    if !(largest.is_finite() && largest > 0.0) {
        return None;
    }
    Some(Mat4::rotation_y(-creature_yaw).transform_dir(delta * (1.0 / largest)))
}

/// Angulo (radianes) entre el frente de reposo de la criatura (+Z del OBJ girado con su yaw) y
/// la direccion del centro al objetivo. `None` sin direccion.
pub fn turn_off_axis(center: Vec3, creature_yaw: f64, target: Vec3) -> Option<f64> {
    let d = rest_direction(center, creature_yaw, target)?;
    Some(d.x.hypot(d.y).atan2(d.z))
}

/// Giro global pedido hacia `target` desde el centro, en el marco de reposo de la criatura
/// (nunca desde su orientacion girada: el objetivo no se realimenta).
///
/// - Mapa angulo-direccion, como `aim_gaze`: `theta` (angulo al frente) en la direccion
///   `(x, y)` del plano tangente. Es continuo en todas partes; a diferencia de un Euler con
///   `atan2`, no salta de +180 a -180 detras. Dentro de la elipse coincide con el Euler hasta
///   segundo orden; el ojo compensa exactamente el resto (ver `rest_frame_target`).
/// - Recortado a la elipse 20/12 y multiplicado por el desvanecido del ojo
///   (`EYE_FOLLOW_FADE_START..EYE_FOLLOW_FADE_END`, 75..105 grados): nunca intenta dar la
///   vuelta hacia algo que esta detras.
///
/// `NEUTRAL` (nunca NaN) sin direccion, con no finitos o con el objetivo sobre el eje.
pub fn aim_turn(center: Vec3, creature_yaw: f64, target: Vec3) -> TurnAngles {
    let Some(d) = rest_direction(center, creature_yaw, target) else {
        return TurnAngles::NEUTRAL;
    };
    let tangential = d.x.hypot(d.y);
    if tangential.is_nan() || tangential <= 1e-12 {
        return TurnAngles::NEUTRAL;
    }
    let theta = tangential.atan2(d.z);
    let t = ((theta - EYE_FOLLOW_FADE_START) / (EYE_FOLLOW_FADE_END - EYE_FOLLOW_FADE_START))
        .clamp(0.0, 1.0);
    let fade = 1.0 - t * t * (3.0 - 2.0 * t);
    if fade <= 0.0 {
        return TurnAngles::NEUTRAL;
    }
    let reach = TurnAngles::clamped(theta * d.x / tangential, theta * d.y / tangential);
    TurnAngles::clamped(reach.yaw * fade, reach.pitch * fade)
}

/// Distancia angular (radianes) por debajo de la cual el estado se fija exactamente en el
/// objetivo (como `EYE_FOLLOW_SNAP_RADIANS`): sin esto nunca volveria al reposo exacto.
pub const DISTANT_TURN_SNAP_RADIANS: f64 = 1e-6;

/// Estado temporal del giro global: encendido/apagado (`H`) y angulos mostrados. Ligero y sin
/// geometria. Se avanza con segundos **reales** (F5 no lo congela).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistantTurn {
    active: bool,
    current: TurnAngles,
}

impl Default for DistantTurn {
    fn default() -> Self {
        Self::new()
    }
}

impl DistantTurn {
    /// Encendido y en reposo.
    pub fn new() -> Self {
        Self {
            active: true,
            current: TurnAngles::NEUTRAL,
        }
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    /// `H`: apagado, el objetivo es el reposo y el estado vuelve con la misma persecucion
    /// (no salta).
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    pub fn current(&self) -> TurnAngles {
        self.current
    }
    /// Objetivo del giro: el pedido `aim` por el peso de distancia (recortado a `[0, 1]`; NaN
    /// da 0) si esta encendido, y el reposo si no. Escalar hacia el centro no sale de la
    /// elipse.
    pub fn goal(&self, aim: TurnAngles, weight: f64) -> TurnAngles {
        if !self.active || weight.is_nan() || weight <= 0.0 {
            return TurnAngles::NEUTRAL;
        }
        if weight >= 1.0 {
            return aim;
        }
        TurnAngles::clamped(aim.yaw * weight, aim.pitch * weight)
    }
    /// Persecucion exponencial hacia `goal`: tras `DISTANT_TURN_T95_SECONDS` reales queda el 5 %
    /// del cambio. Independiente de como se trocee el tiempo (salvo el ajuste final de
    /// `DISTANT_TURN_SNAP_RADIANS`), sin pasarse y, por convexidad, dentro de la elipse.
    /// `elapsed` cero, negativo o no finito no avanza.
    pub fn update(&mut self, goal: TurnAngles, elapsed: f64) {
        if !(elapsed.is_finite() && elapsed > 0.0) {
            return;
        }
        let keep = (-(20.0_f64.ln()) / DISTANT_TURN_T95_SECONDS * elapsed).exp();
        let yaw = goal.yaw + (self.current.yaw - goal.yaw) * keep;
        let pitch = goal.pitch + (self.current.pitch - goal.pitch) * keep;
        self.current = if (yaw - goal.yaw).hypot(pitch - goal.pitch) <= DISTANT_TURN_SNAP_RADIANS {
            goal
        } else {
            TurnAngles::clamped(yaw, pitch)
        };
    }
    /// Vuelve al reposo exacto (presets); conserva encendido/apagado.
    pub fn reset(&mut self) {
        self.current = TurnAngles::NEUTRAL;
    }
}

/// Euler de `Transform` (`(x, y, z)`, `R = Ry * Rx * Rz`, ver `Mat4::from_euler`) del modelo
/// girado: `R = Ry(creature_yaw + yaw) * Rx(-pitch)`. Primero el cabeceo en el marco del OBJ
/// (`Rx(-pitch)` lleva +Z hacia +Y con pitch positivo), luego el yaw total alrededor de +Y del
/// mundo. Sin alabeo: el giro nunca inclina la criatura de lado.
pub fn turned_euler(creature_yaw: f64, angles: TurnAngles) -> Vec3 {
    Vec3::new(-angles.pitch, creature_yaw + angles.yaw, 0.0)
}

/// Parte lineal del giro en mundo alrededor del centro: `G = R_girado * R_reposo^T =
/// Ry(creature_yaw + yaw) * Rx(-pitch) * Ry(-creature_yaw)`. Un punto de reposo `p` queda en
/// `centro + G (p - centro)`.
pub fn world_turn(creature_yaw: f64, angles: TurnAngles) -> Mat4 {
    Mat4::rotation_y(creature_yaw + angles.yaw)
        * Mat4::rotation_x(-angles.pitch)
        * Mat4::rotation_y(-creature_yaw)
}

/// Inversa analitica de `world_turn` (rotaciones conocidas: los mismos factores en orden
/// inverso con angulos opuestos, es decir su traspuesta), sin invertir matrices generales.
pub fn world_turn_inverse(creature_yaw: f64, angles: TurnAngles) -> Mat4 {
    Mat4::rotation_y(creature_yaw)
        * Mat4::rotation_x(angles.pitch)
        * Mat4::rotation_y(-(creature_yaw + angles.yaw))
}

/// Punto del marco de reposo equivalente a `target`: `centro + G^-1 (target - centro)`. El ojo
/// apunta con su calibracion de reposo a este punto y, girado con la criatura, mira al
/// objetivo real. En reposo devuelve `target` exacto (rutas de siempre, sin redondeo nuevo).
pub fn rest_frame_target(
    center: Vec3,
    creature_yaw: f64,
    angles: TurnAngles,
    target: Vec3,
) -> Vec3 {
    if angles.is_neutral() {
        return target;
    }
    center + world_turn_inverse(creature_yaw, angles).transform_dir(target - center)
}

/// Transform de la criatura girada alrededor del centro fijo de su colocacion: misma escala
/// uniforme, rotacion `turned_euler` y la traslacion que deja el centro envolvente del modelo
/// en `placement.center()` (como `CreaturePlacement::transform_for`). No copia ni toca
/// vertices, normales ni UV: solo cambia la matriz con la que se dibujan. En reposo devuelve
/// el transform original exacto.
pub fn turned_transform(creature: &StaticCreature, angles: TurnAngles) -> Transform {
    let base = creature.transform();
    if angles.is_neutral() {
        return base;
    }
    let placement = creature.placement();
    let rotation = turned_euler(placement.yaw(), angles);
    let model_center = Vec3::from_array(creature.model().bounds_center);
    Transform {
        position: placement.center()
            - Mat4::from_euler(rotation).transform_dir(model_center.mul_elem(base.scale)),
        rotation,
        scale: base.scale,
    }
}

/// Distancia (radios) y angulo lateral del preset `LEJOS` de la candidata.
pub const FAR_PRESET_RADII: f64 = 8.0;
pub const FAR_PRESET_LATERAL: f64 = 25.0 * DEGREE;

/// Arranques explicitos de la candidata (teclas `7` y `8`; `R` repite el ultimo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreaturePreset {
    /// `--near-creature`: de frente a `SPAWN_DISTANCE_RADII` radios.
    Near,
    /// A `FAR_PRESET_RADII` radios y `FAR_PRESET_LATERAL` hacia +X del frente.
    Far,
}

/// Posicion y direccion de la nave del preset, mirando al centro y sin elevacion. `Near` es
/// exactamente `CreaturePlacement::spawn_pose`. La camara de persecucion queda detras de la
/// nave (`World::chase_distance`), un poco mas lejos que la nave.
pub fn preset_pose(placement: &CreaturePlacement, preset: CreaturePreset) -> (Vec3, Vec3) {
    match preset {
        CreaturePreset::Near => placement.spawn_pose(),
        CreaturePreset::Far => {
            let (sin, cos) = (placement.yaw() + FAR_PRESET_LATERAL).sin_cos();
            let direction = Vec3::new(sin, 0.0, cos);
            (
                placement.center() + direction * (FAR_PRESET_RADII * placement.world_radius()),
                -direction,
            )
        }
    }
}

/// Lo que muestra el panel del giro global (QA/HUD). Los campos opcionales son `None` sin
/// criatura.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistantTurnStatus {
    pub turn: DistantTurn,
    /// Pedido hacia el punto elegido, antes del peso de distancia.
    pub aim: Option<TurnAngles>,
    /// Objetivo actual (`DistantTurn::goal`): el pedido por el peso, o el reposo si esta apagado.
    pub goal: Option<TurnAngles>,
    /// Distancia de la camara al centro, en radios, y su peso.
    pub distance_radii: Option<f64>,
    pub weight: Option<f64>,
    /// Angulo (radianes) entre el frente de reposo y el punto elegido (`turn_off_axis`).
    pub off_axis: Option<f64>,
}

/// Celda de la fuente 5x7 de `draw_text` (privada en `overlay.rs`; como en la comparacion).
const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// Lineas ASCII en mayusculas (fuente 5x7, como la comparacion), de 44 caracteres como mucho:
/// `H`, presets, distancia y peso, angulos actual y objetivo del giro y el estado del pedido
/// (antes del peso) respecto de la elipse 20/12 y del desvanecido 75..105.
pub fn distant_turn_lines(status: &DistantTurnStatus, last_preset: CreaturePreset) -> Vec<String> {
    // `+ 0.0` convierte -0.0 en 0.0 para no mostrar "-0.0".
    let deg = |r: f64| r.to_degrees() + 0.0;
    let angles = |t: TurnAngles| format!("YAW {:+.1}  PITCH {:+.1}", deg(t.yaw), deg(t.pitch));
    let mut lines = vec![
        if status.turn.is_active() {
            "GIRO GLOBAL: ON   H APAGA   T95 8 S".to_string()
        } else {
            "GIRO GLOBAL: OFF  H ENCIENDE (VUELVE SUAVE)".to_string()
        },
        format!(
            "7 CERCA  8 LEJOS  R REPITE {}",
            match last_preset {
                CreaturePreset::Near => "CERCA",
                CreaturePreset::Far => "LEJOS",
            }
        ),
    ];
    let (Some(aim), Some(goal), Some(distance), Some(weight)) = (
        status.aim,
        status.goal,
        status.distance_radii,
        status.weight,
    ) else {
        lines.push("SIN CRIATURA: SIN GIRO".into());
        return lines;
    };
    lines.push(format!(
        "CAMARA A {distance:.1} RADIOS  PESO {weight:.2} (3-7)"
    ));
    lines.push(format!("GLOBAL ACTUAL   {}", angles(status.turn.current())));
    lines.push(format!("GLOBAL OBJETIVO {}", angles(goal)));
    let state = match status.off_axis {
        None => "SIN DIRECCION",
        Some(theta) if theta >= EYE_FOLLOW_FADE_END => "REPOSO",
        Some(theta) if theta > EYE_FOLLOW_FADE_START => "DESVANECE",
        Some(_)
            if (aim.yaw / DISTANT_TURN_MAX_YAW).hypot(aim.pitch / DISTANT_TURN_MAX_PITCH)
                >= 1.0 - 1e-9 =>
        {
            "TOPE"
        }
        Some(_) => "DENTRO",
    };
    let front = status
        .off_axis
        .map_or(String::new(), |t| format!("  FRENTE {:.1} GRADOS", deg(t)));
    lines.push(format!("LIMITE 20/12: {state}{front}"));
    lines
}

/// Panel abajo a la derecha, junto al de la comparacion (`comparison`, abajo a la izquierda),
/// con su misma fuente y escala. Si los dos no caben uno al lado del otro, va encima del de la
/// comparacion, a la izquierda. El tamano se recorta al framebuffer.
pub fn distant_turn_panel(
    width: usize,
    height: usize,
    lines: &[String],
    comparison: &PanelLayout,
) -> PanelLayout {
    let scale = comparison.scale;
    let text = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) * (GLYPH_W + 1);
    let w = (text * scale + 16).min(width);
    let h = (lines.len() * (GLYPH_H + 3) * scale + 12).min(height);
    if comparison.x + comparison.w + w <= width {
        PanelLayout {
            x: width - w,
            y: height - h,
            w,
            h,
            scale,
        }
    } else {
        let h = h.min(comparison.y);
        PanelLayout {
            x: 0,
            y: comparison.y - h,
            w,
            h,
            scale,
        }
    }
}

/// Dibuja el panel del giro (oscurece su rectangulo y escribe las lineas) con `darken_rect` y
/// `draw_text`, sin tocar nada fuera del rectangulo ni el z-buffer.
pub fn draw_distant_turn_panel(fb: &mut Framebuffer, lines: &[String], comparison: &PanelLayout) {
    let panel = distant_turn_panel(fb.width, fb.height, lines, comparison);
    fb.darken_rect(panel.x, panel.y, panel.w, panel.h, 0.55);
    let line_h = (GLYPH_H + 3) * panel.scale;
    for (i, line) in lines.iter().enumerate() {
        let y = panel.y + 6 + i * line_h;
        if y + GLYPH_H * panel.scale <= panel.y + panel.h {
            draw_text(fb, panel.x + 8, y, panel.scale, 0xE8F0F0, line);
        }
    }
}
