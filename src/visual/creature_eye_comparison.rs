//! Comparacion A/B (debug, opt-in) del objetivo de la mirada: nave o camara. Solo cambia a
//! que punto mira el ojo; limites, desvanecido, tiempo y geometria son los de
//! `creature_eye_follow`. Aqui viven el diagnostico puro y el texto del panel. Ver
//! `docs/static-creature.md`.

use crate::{
    debug::overlay::draw_text,
    math::{Mat4, Vec3},
    renderer::framebuffer::Framebuffer,
    visual::creature_eye_follow::{
        EYE_FOLLOW_FADE_END, EYE_FOLLOW_FADE_START, EYE_FOLLOW_MAX_PITCH, EYE_FOLLOW_MAX_YAW,
        GazeAngles,
    },
};

/// Combinaciones de `--creature-eye-compare`, sin tocar archivos. Se valida despues de
/// `validate_eye_follow_flags`: sus errores (y los de las etapas) se conservan.
pub fn validate_eye_compare_flags(
    compare: bool,
    eye_follow: bool,
    legacy: bool,
) -> Result<(), String> {
    if !compare {
        return Ok(());
    }
    if legacy {
        return Err("--legacy no admite --creature-eye-compare".into());
    }
    if !eye_follow {
        return Err("--creature-eye-compare necesita --creature-eye-follow".into());
    }
    Ok(())
}

/// Celda de la fuente 5x7 de `draw_text` (sin contar el espacio entre letras).
const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// Punto al que mira el ojo. `Ship` es el comportamiento normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EyeTarget {
    /// Posicion de la nave.
    #[default]
    Ship,
    /// Posicion de la camara de persecucion (nunca el punto al que mira la camara).
    Camera,
}

/// Estado de la mirada respecto de sus limites, para el panel de la comparacion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GazeLimit {
    /// Dentro de la elipse, sin desvanecido.
    Inside,
    /// En el borde de la elipse 10/8 (el pedido la excede): "TOPE".
    Clamped,
    /// Entre 75 y 105 grados del eje: el desvanecido acerca la mirada al reposo.
    Fading,
    /// A 105 grados o mas del eje: reposo.
    Rest,
    /// Objetivo sobre el pivote o datos no finitos: `aim_gaze` da reposo.
    NoDirection,
}

/// Angulo (grados) entre el eje optico de la criatura en mundo y la direccion del pivote al
/// objetivo: el mismo que usa el desvanecido de `aim_gaze` (girar con el yaw de la criatura
/// conserva angulos). `None` sin direccion (objetivo en el pivote, eje nulo o no finitos).
pub fn off_axis_degrees(
    pivot_world: Vec3,
    optical_axis: Vec3,
    creature_yaw: f64,
    target_world: Vec3,
) -> Option<f64> {
    if !(pivot_world.is_finite()
        && optical_axis.is_finite()
        && creature_yaw.is_finite()
        && target_world.is_finite())
    {
        return None;
    }
    let delta = target_world - pivot_world;
    let largest = delta.x.abs().max(delta.y.abs()).max(delta.z.abs());
    let axis = Mat4::rotation_y(creature_yaw).transform_dir(optical_axis.normalized());
    if !(largest.is_finite() && largest > 0.0) || axis == Vec3::ZERO {
        return None;
    }
    let d = delta * (1.0 / largest);
    Some(axis.cross(d).length().atan2(axis.dot(d)).to_degrees())
}

/// Clasifica el objetivo angular `goal` con el angulo al eje, con los umbrales de
/// `aim_gaze`: desvanecido y reposo mandan sobre el tope.
pub fn gaze_limit(off_axis_degrees: Option<f64>, goal: GazeAngles) -> GazeLimit {
    let Some(theta) = off_axis_degrees else {
        return GazeLimit::NoDirection;
    };
    let theta = theta.to_radians();
    if theta >= EYE_FOLLOW_FADE_END {
        return GazeLimit::Rest;
    }
    if theta > EYE_FOLLOW_FADE_START {
        return GazeLimit::Fading;
    }
    let edge = (goal.yaw() / EYE_FOLLOW_MAX_YAW).hypot(goal.pitch() / EYE_FOLLOW_MAX_PITCH);
    if edge >= 1.0 - 1e-9 {
        GazeLimit::Clamped
    } else {
        GazeLimit::Inside
    }
}

/// Lo que muestra el panel: el punto elegido, los angulos actuales, el objetivo angular
/// (`None` sin criatura) y el angulo al eje del objetivo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeComparisonStatus {
    pub target: EyeTarget,
    pub current: GazeAngles,
    pub goal: Option<GazeAngles>,
    pub off_axis_degrees: Option<f64>,
}

/// Lineas ASCII en mayusculas para la fuente 5x7 del overlay (sin acentos ni simbolos que no
/// dibuja). Grados con un decimal; el estado no oculta el tope ni el desvanecido.
pub fn eye_comparison_lines(status: &EyeComparisonStatus) -> Vec<String> {
    let name = |t: EyeTarget| match t {
        EyeTarget::Ship => "NAVE",
        EyeTarget::Camera => "CAMARA",
    };
    let other = match status.target {
        EyeTarget::Ship => EyeTarget::Camera,
        EyeTarget::Camera => EyeTarget::Ship,
    };
    // `+ 0.0` convierte -0.0 en 0.0 para no mostrar "-0.0".
    let deg = |r: f64| r.to_degrees() + 0.0;
    let angles = |g: GazeAngles| format!("YAW {:+.1}  PITCH {:+.1}", deg(g.yaw()), deg(g.pitch()));
    let mut lines = vec![format!(
        "MIRADA A: {}   G CAMBIA A {}",
        name(status.target),
        name(other)
    )];
    let Some(goal) = status.goal else {
        lines.push("SIN CRIATURA: SIN OBJETIVO".into());
        return lines;
    };
    lines.push(format!("ACTUAL   {}", angles(status.current)));
    lines.push(format!("OBJETIVO {}", angles(goal)));
    let state = match gaze_limit(status.off_axis_degrees, goal) {
        GazeLimit::Inside => "DENTRO",
        GazeLimit::Clamped => "TOPE",
        GazeLimit::Fading => "DESVANECE",
        GazeLimit::Rest => "REPOSO",
        GazeLimit::NoDirection => "SIN DIRECCION",
    };
    let axis = status.off_axis_degrees.map_or(String::new(), |t| {
        format!("   EJE {t:.1} GRADOS (FADE 75-105)")
    });
    lines.push(format!("LIMITE 10/8: {state}{axis}"));
    lines
}

/// Rectangulo del panel inferior y escala de la fuente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelLayout {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub scale: usize,
}

/// Panel abajo a la izquierda, separado de la banda superior del overlay (que no cambia):
/// misma fuente 5x7 y misma escala que el overlay (2 desde 1000 pixeles de ancho). El ancho se
/// recorta al framebuffer.
pub fn eye_comparison_panel(width: usize, height: usize, lines: &[String]) -> PanelLayout {
    let scale = if width >= 1000 { 2 } else { 1 };
    let text = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) * (GLYPH_W + 1);
    let h = (lines.len() * (GLYPH_H + 3) * scale + 12).min(height);
    PanelLayout {
        x: 0,
        y: height - h,
        w: (text * scale + 16).min(width),
        h,
        scale,
    }
}

/// Dibuja el panel (oscurece su rectangulo y escribe las lineas) con `darken_rect` y
/// `draw_text`; no toca nada fuera del rectangulo ni el z-buffer.
pub fn draw_eye_comparison_panel(fb: &mut Framebuffer, lines: &[String]) {
    let panel = eye_comparison_panel(fb.width, fb.height, lines);
    fb.darken_rect(panel.x, panel.y, panel.w, panel.h, 0.55);
    let line_h = (GLYPH_H + 3) * panel.scale;
    for (i, line) in lines.iter().enumerate() {
        draw_text(
            fb,
            panel.x + 8,
            panel.y + 6 + i * line_h,
            panel.scale,
            0xE8F0F0,
            line,
        );
    }
}
