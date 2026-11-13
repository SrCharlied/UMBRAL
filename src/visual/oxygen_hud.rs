//! HUD de RESERVA de oxigeno (preview opt-in `--oxygen-hud`, hud_oxygen_01).
//!
//! Un instrumento oscuro "hundido" en el borde superior: un disco centrado POR ENCIMA de la pantalla del que
//! solo asoma el casquete inferior. Su arco inferior es la reserva (se vacia en un solo sentido, de derecha a
//! izquierda, con tres marcas tenues en 25/50/75 %) y la lectura `O2 xx%` va dentro del disco. Debajo queda
//! un hueco RESERVADO para un ojo futuro (`eye_reserve`) que este modulo nunca dibuja.
//!
//! Autoridad del dato: la RESERVA REAL del jugador (`World::gameplay.oxygen01`), nunca el O2 visual
//! (`--debug-oxygen`, transicion de la percepcion); en la experiencia por defecto las teclas 1-4/M/N arrastran
//! tambien la reserva real y el HUD las sigue. Arco y numero salen de la MISMA `OxygenReading` del mismo
//! cuadro. Dibujar no gasta ni recupera nada: solo escribe pixeles dentro de `OxygenHudLayout::bounds`, sin
//! profundidad. En el juego va DESPUES de la percepcion (legible con O2 bajo), bajo los marcadores y el final.

use crate::{debug::overlay::draw_text, renderer::framebuffer::Framebuffer, scene::world::World};

/// Arco lleno (vidrio de mar palido) y arco vacio (pizarra oscura).
pub const HUD_FILL: u32 = 0x9cc7bf;
pub const HUD_TRACK: u32 = 0x1d2731;
/// Lectura `O2 xx%` (mismo hueso claro que el texto existente del juego).
pub const HUD_TEXT: u32 = 0xd9e4e6;
/// Cuerpo del disco (grafito azulado casi opaco) y borde de metal gastado.
const HUD_BODY: u32 = 0x0b1118;
const HUD_RIM: u32 = 0x56606a;
const BODY_ALPHA: f64 = 0.9;
/// Sombra de contacto fuera del borde: oscurece el mundo, no lo tapa.
const SHADOW_DARKEN: f64 = 0.45;
/// Las marcas oscurecen el arco (lleno o vacio) en lugar de pintar otro color.
const TICK_KEEP: f64 = 0.55;
const TICKS: [f64; 3] = [0.25, 0.5, 0.75];

/// Geometria en unidades de `scale` (1 a 320x180).
const RADIUS: f64 = 48.0;
/// Profundidad visible del disco desde el borde superior.
const DEPTH: f64 = 22.0;
const RIM: f64 = 1.0;
const TRACK: [f64; 2] = [43.0, 46.0];
const SHADOW: f64 = 1.5;
const HALF_SPAN_DEGREES: f64 = 42.0;
const LABEL_TOP: f64 = 3.0;
/// Margen lateral minimo para dibujar el instrumento.
const SIDE_MARGIN: usize = 2;
/// Hueco del ojo futuro: separacion bajo el instrumento y tamano.
const EYE_GAP: usize = 2;
const EYE_SIZE: [usize; 2] = [28, 16];
/// Ancho de cada caracter de `draw_text` (5 columnas + 1 de separacion).
const GLYPH_ADVANCE: usize = 6;

/// Opcion de preview del juego: HUD encendido y, solo para previews, la reserva REAL inicial.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OxygenHudOptions {
    pub enabled: bool,
    pub initial_reserve: Option<f64>,
}

/// HUD efectivo del juego: encendido con la experiencia completa por defecto (ventana sin `--classic`) o con
/// `--oxygen-hud`; conserva la reserva inicial pedida.
pub fn oxygen_hud_for_game(flags: OxygenHudOptions, default_effects: bool) -> OxygenHudOptions {
    OxygenHudOptions {
        enabled: flags.enabled || default_effects,
        ..flags
    }
}

/// Combinaciones de `--oxygen-hud` / `--oxygen-reserve`, sin tocar archivos. `debug_oxygen`: el O2 VISUAL
/// viene de `--debug-oxygen` (o de la fuente legada), asi que la reserva real inicial no se combina con el.
pub fn validate_oxygen_hud_flags(
    options: OxygenHudOptions,
    legacy: bool,
    debug_oxygen: bool,
) -> Result<(), String> {
    if options.enabled && legacy {
        return Err(
            "--oxygen-hud no se combina con --legacy (el visor legado no usa el compositor del juego)"
                .into(),
        );
    }
    let Some(reserve) = options.initial_reserve else {
        return Ok(());
    };
    if !options.enabled {
        return Err("--oxygen-reserve requiere --oxygen-hud (preview del HUD de oxigeno)".into());
    }
    if debug_oxygen {
        return Err(
            "--oxygen-reserve fija la reserva REAL: no se combina con --debug-oxygen (O2 visual)"
                .into(),
        );
    }
    OxygenReading::new(reserve)
        .map(|_| ())
        .map_err(|_| format!("--oxygen-reserve necesita una reserva en [0, 1]: {reserve}"))
}

/// Valor de `--oxygen-reserve`: numero finito en `[0, 1]`.
pub fn parse_oxygen_reserve(text: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| OxygenReading::new(*v).is_ok())
        .ok_or_else(|| format!("--oxygen-reserve necesita una reserva en [0, 1]: {text}"))
}

/// Reserva de O2 validada para el HUD: finita y en `[0, 1]`. Arco y numero usan siempre la misma.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OxygenReading {
    fraction: f64,
}

impl OxygenReading {
    /// `Err` (sin efectos) con un valor no finito o fuera de `[0, 1]`.
    pub fn new(oxygen01: f64) -> Result<Self, String> {
        if oxygen01.is_finite() && (0.0..=1.0).contains(&oxygen01) {
            Ok(Self { fraction: oxygen01 })
        } else {
            Err(format!(
                "HUD de oxigeno: reserva no finita o fuera de [0, 1]: {oxygen01}"
            ))
        }
    }

    /// La reserva REAL (`world.gameplay.oxygen01`), no el O2 visual de depuracion o de la percepcion. Durante la
    /// muerte por el monstruo (`World::dying`) es 0: la secuencia sostiene el O2 en 3 % solo como mecanica.
    pub fn real_reserve(world: &World) -> Result<Self, String> {
        let reserve = Self::new(world.gameplay.oxygen01)?;
        Ok(if world.dying {
            Self { fraction: 0.0 }
        } else {
            reserve
        })
    }

    pub fn fraction(self) -> f64 {
        self.fraction
    }

    /// Porcentaje entero: `0` solo con la reserva vacia y `100` solo con la reserva llena; cualquier reserva
    /// positiva muestra al menos 1 y cualquiera incompleta como mucho 99 (redondeo al entero mas cercano).
    pub fn percent(self) -> u32 {
        if self.fraction <= 0.0 {
            0
        } else if self.fraction >= 1.0 {
            100
        } else {
            ((self.fraction * 100.0).round() as u32).clamp(1, 99)
        }
    }

    pub fn label(self) -> String {
        format!("O2 {}%", self.percent())
    }
}

/// Rectangulo de pixeles `[x0, x1) x [y0, y1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudRect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl HudRect {
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
    pub fn width(&self) -> usize {
        self.x1 - self.x0
    }
    pub fn height(&self) -> usize {
        self.y1 - self.y0
    }
}

/// Geometria del instrumento para un tamano de cuadro (coordenadas de pixel; el centro del pixel `(x, y)`
/// es `(x + 0.5, y + 0.5)`; los angulos se miden desde ABAJO, negativos a la izquierda).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OxygenHudLayout {
    /// Factor entero de tamano (y de la fuente 5x7): 1 hasta 1279x719, 2 a 1280x720, 3 a 1920x1080...
    pub scale: usize,
    pub frame_width: usize,
    /// Centro del disco; `center[1] < 0`: por encima del borde superior.
    pub center: [f64; 2],
    /// Radio exterior del disco (sin la sombra) y grosor del borde.
    pub radius: f64,
    pub rim: f64,
    /// Radios interior y exterior del arco de reserva.
    pub track: [f64; 2],
    /// Angulo de cada extremo del arco desde abajo.
    pub half_span: f64,
    /// Todo pixel que el HUD puede escribir.
    pub bounds: HudRect,
    pub label_top: usize,
    /// Hueco reservado bajo el instrumento para el ojo futuro: NUNCA se dibuja.
    pub eye_reserve: HudRect,
}

impl OxygenHudLayout {
    /// Angulo donde termina lo lleno: `-half_span` vacio, `+half_span` lleno, lineal en la reserva.
    pub fn fill_end_angle(&self, reading: OxygenReading) -> f64 {
        -self.half_span + 2.0 * self.half_span * reading.fraction()
    }

    /// Esquina superior izquierda de `text` centrado bajo el disco con la fuente existente.
    pub fn label_origin(&self, text: &str) -> [usize; 2] {
        let width = (text.chars().count() * GLYPH_ADVANCE).saturating_sub(1) * self.scale;
        [self.frame_width.saturating_sub(width) / 2, self.label_top]
    }
}

/// Layout para `width x height`, o `None` si el instrumento y el hueco del ojo no caben (no se dibuja nada).
pub fn oxygen_hud_layout(width: usize, height: usize) -> Option<OxygenHudLayout> {
    let start = (width / 640).min(height / 360).max(1);
    (1..=start)
        .rev()
        .find_map(|scale| layout_at(width, height, scale))
}

fn layout_at(width: usize, height: usize, scale: usize) -> Option<OxygenHudLayout> {
    let k = scale as f64;
    let (radius, shadow) = (RADIUS * k, SHADOW * k);
    let center = [width as f64 / 2.0, (DEPTH - RADIUS) * k];
    // Fila mas ancha visible: el borde superior (el centro queda por encima).
    let half = ((radius + shadow).powi(2) - center[1].powi(2))
        .sqrt()
        .ceil() as usize;
    if half + SIDE_MARGIN > width / 2 {
        return None;
    }
    let bounds_x0 = width / 2 - half;
    let bounds = HudRect {
        x0: bounds_x0,
        y0: 0,
        x1: width - bounds_x0,
        y1: (center[1] + radius + shadow).ceil() as usize,
    };
    let (eye_w, eye_h) = (EYE_SIZE[0] * scale, EYE_SIZE[1] * scale);
    let eye_x0 = (width / 2).checked_sub(eye_w / 2)?;
    let eye_y0 = bounds.y1 + EYE_GAP * scale;
    let eye_reserve = HudRect {
        x0: eye_x0,
        y0: eye_y0,
        x1: width - eye_x0,
        y1: eye_y0 + eye_h,
    };
    if eye_reserve.y1 > height || bounds.y1 > height {
        return None;
    }
    Some(OxygenHudLayout {
        scale,
        frame_width: width,
        center,
        radius,
        rim: RIM * k,
        track: [TRACK[0] * k, TRACK[1] * k],
        half_span: HALF_SPAN_DEGREES.to_radians(),
        bounds,
        label_top: (LABEL_TOP * k) as usize,
        eye_reserve,
    })
}

fn rgb(c: u32) -> [f64; 3] {
    [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff].map(|v| v as f64)
}

fn pack(c: [f64; 3]) -> u32 {
    let ch = |v: f64| (v.round().clamp(0.0, 255.0)) as u32;
    (ch(c[0]) << 16) | (ch(c[1]) << 8) | ch(c[2])
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Color de UNA submuestra `(sx, sy)` sobre el pixel del mundo `base`.
fn sample(
    l: &OxygenHudLayout,
    reading: OxygenReading,
    base: [f64; 3],
    sx: f64,
    sy: f64,
) -> [f64; 3] {
    let (dx, dy) = (sx - l.center[0], sy - l.center[1]);
    let r = dx.hypot(dy);
    let shadow = SHADOW * l.scale as f64;
    if r > l.radius + shadow {
        return base;
    }
    if r > l.radius {
        let t = 1.0 - (r - l.radius) / shadow;
        return base.map(|v| v * (1.0 - SHADOW_DARKEN * t));
    }
    if r > l.radius - l.rim {
        return rgb(HUD_RIM);
    }
    let phi = dx.atan2(dy);
    if r >= l.track[0] && r <= l.track[1] && phi.abs() <= l.half_span {
        let full = reading.fraction() > 0.0 && phi <= l.fill_end_angle(reading);
        let color = rgb(if full { HUD_FILL } else { HUD_TRACK });
        let tick = TICKS.iter().any(|t| {
            let angle = -l.half_span + 2.0 * l.half_span * t;
            (phi - angle).abs() * r <= 0.5 * l.scale as f64
        });
        return if tick {
            color.map(|v| v * TICK_KEEP)
        } else {
            color
        };
    }
    mix(base, rgb(HUD_BODY), BODY_ALPHA)
}

/// Dibuja el instrumento con la lectura dada. Sin layout (cuadro demasiado pequeno) no hace nada. Solo
/// escribe pixeles dentro de `bounds` (bordes suavizados con 2x2 submuestras) y nunca la profundidad.
pub fn draw_oxygen_hud(framebuffer: &mut Framebuffer, reading: OxygenReading) {
    let Some(l) = oxygen_hud_layout(framebuffer.width, framebuffer.height) else {
        return;
    };
    const SUB: [[f64; 2]; 4] = [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]];
    let width = framebuffer.width;
    for y in l.bounds.y0..l.bounds.y1 {
        for x in l.bounds.x0..l.bounds.x1 {
            let index = y * width + x;
            let base = rgb(framebuffer.pixels[index]);
            let mut sum = [0.0; 3];
            for [ox, oy] in SUB {
                let c = sample(&l, reading, base, x as f64 + ox, y as f64 + oy);
                for i in 0..3 {
                    sum[i] += c[i] / SUB.len() as f64;
                }
            }
            framebuffer.pixels[index] = pack(sum);
        }
    }
    let label = reading.label();
    let [lx, ly] = l.label_origin(&label);
    draw_text(framebuffer, lx, ly, l.scale, HUD_TEXT, &label);
}
