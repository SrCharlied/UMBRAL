//! Presentacion final de la partida (victoria y muerte): transicion sobre el ultimo cuadro y
//! cierre narrativo. Solo presentacion: la logica de ganar/perder sigue en `GameplayState`.
//!
//! El reloj del final cuenta segundos **reales** desde que la partida entro en estado terminal
//! (`EndingClock`); el mundo esta congelado, asi que el cuadro de fondo es estable y este modulo
//! solo lo recolorea y escribe encima. `Shift+R` (nueva partida) saca del estado terminal y
//! reinicia el reloj.

use crate::{
    debug::overlay::draw_text, gameplay::GameplayState, renderer::framebuffer::Framebuffer,
};

/// Segundos de la transicion de color sobre la escena.
pub const FADE_SECONDS: f64 = 2.6;
/// Segundos antes de empezar a escribir el texto.
pub const TEXT_DELAY: f64 = 1.4;
/// Letras por segundo del cierre narrativo.
pub const CHARS_PER_SECOND: f64 = 38.0;
/// Pausa tras terminar el texto antes de mostrar la invitacion a reiniciar.
pub const PROMPT_DELAY: f64 = 0.8;
/// Segundos suficientes para ver el final completo (headless y capturas).
pub const FULLY_SHOWN: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndingKind {
    Death,
    Victory,
}

impl EndingKind {
    /// La muerte prevalece, igual que en `GameplayState`.
    pub fn of(gameplay: &GameplayState) -> Option<Self> {
        if gameplay.game_over {
            Some(Self::Death)
        } else if gameplay.objective_complete {
            Some(Self::Victory)
        } else {
            None
        }
    }
}

/// Segundos reales en estado terminal; vuelve a cero al salir de el.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EndingClock {
    seconds: f64,
    active: bool,
}

impl EndingClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// `elapsed` en segundos reales; valores no finitos o negativos no avanzan. Un cuadro
    /// avanza como mucho 0.25 s para que una pausa de la ventana no salte la transicion.
    pub fn update(&mut self, gameplay: &GameplayState, elapsed: f64) {
        if EndingKind::of(gameplay).is_none() {
            *self = Self::default();
            return;
        }
        if !self.active {
            self.active = true;
            self.seconds = 0.0;
            return;
        }
        if elapsed.is_finite() && elapsed > 0.0 {
            self.seconds = (self.seconds + elapsed.min(0.25)).min(FULLY_SHOWN * 10.0);
        }
    }

    /// Segundos desde el final, o `None` si la partida sigue.
    pub fn seconds(&self) -> Option<f64> {
        self.active.then_some(self.seconds)
    }
}

/// Datos de la partida que el cierre muestra.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndingSummary {
    pub kind: EndingKind,
    pub oxygen01: f64,
    pub resources: u32,
    pub resources_total: usize,
    pub logs: u32,
    pub logs_total: usize,
    pub mission_seconds: f64,
}

impl EndingSummary {
    pub fn from_gameplay(gameplay: &GameplayState, mission_seconds: f64) -> Option<Self> {
        Some(Self {
            kind: EndingKind::of(gameplay)?,
            oxygen01: gameplay.oxygen01,
            resources: gameplay.collected_resources,
            resources_total: gameplay.resources.len(),
            logs: gameplay.collected_logs,
            logs_total: gameplay.logs.len(),
            mission_seconds,
        })
    }
}

/// Titulo, parrafos narrativos y linea de estadisticas. Solo caracteres de la fuente 5x7
/// (sin tildes ni signos de apertura).
pub fn ending_text(summary: &EndingSummary) -> (String, Vec<String>, String) {
    let missing = summary.logs_total.saturating_sub(summary.logs as usize);
    let (title, lines) = match summary.kind {
        EndingKind::Death => (
            "SIN OXIGENO",
            vec![
                "El aire se acabo antes que el camino.".to_string(),
                if missing == 0 {
                    "Llevabas los tres registros. El Sol estaba tan cerca.".to_string()
                } else if missing == 1 {
                    "Un registro quedo flotando en la oscuridad, sin nadie que lo lea.".to_string()
                } else {
                    format!(
                        "{missing} registros quedaron flotando en la oscuridad, sin nadie que los lea."
                    )
                },
                "Cuando se apago el ultimo aliento, el vacio dejo de esconderse.".to_string(),
                "Y por fin pudiste verlo.".to_string(),
            ],
        ),
        EndingKind::Victory => (
            "EL UMBRAL",
            vec![
                "Los tres registros apuntaban aqui, y entraste en la luz.".to_string(),
                format!(
                    "Llegaste con {:.0}% de oxigeno: lo justo para no ver demasiado.",
                    summary.oxygen01 * 100.0
                ),
                "La voz de la antena era la tuya. Las costillas eran de tu nave.".to_string(),
                "El Sol abre el ojo, y esta vez no aparta la mirada.".to_string(),
            ],
        ),
    };
    let total = summary.mission_seconds.max(0.0).round() as u64;
    let stats = format!(
        "TIEMPO {:02}:{:02}  |  TANQUES DE O2 {}  |  REGISTROS {}/{}",
        total / 60,
        total % 60,
        summary.resources,
        summary.logs,
        summary.logs_total
    );
    (title.to_string(), lines, stats)
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn unpack(c: u32) -> [f64; 3] {
    [16, 8, 0].map(|s| f64::from((c >> s) & 0xff) / 255.0)
}

fn pack(c: [f64; 3]) -> u32 {
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    (ch(c[0]) << 16) | (ch(c[1]) << 8) | ch(c[2])
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Recolorea el cuadro de la escena segun el tipo de final y el tiempo transcurrido.
fn apply_transition(fb: &mut Framebuffer, kind: EndingKind, seconds: f64) {
    let (w, h) = (fb.width as f64, fb.height as f64);
    let half_diag = (w * w + h * h).sqrt() * 0.5;
    let t = smoothstep(0.0, FADE_SECONDS, seconds);
    match kind {
        EndingKind::Death => {
            // Desaturar, enrojecer y cerrar un iris hacia el centro: la vista se apaga.
            // Radio relativo a la semidiagonal: en t = 0 queda fuera del cuadro (escena intacta).
            let iris = 1.3 - 0.95 * t;
            let soft = 0.25;
            for y in 0..fb.height {
                for x in 0..fb.width {
                    let dx = x as f64 + 0.5 - w * 0.5;
                    let dy = y as f64 + 0.5 - h * 0.5;
                    let r = (dx * dx + dy * dy).sqrt() / half_diag;
                    let p = &mut fb.pixels[y * fb.width + x];
                    let c = unpack(*p);
                    let lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    let grey = [lum * 1.1 + 0.03, lum * 0.55, lum * 0.5];
                    let faded = mix(c, grey, t);
                    let dim = 1.0 - 0.55 * t;
                    let open = 1.0 - smoothstep(iris - soft, iris, r);
                    *p = pack(faded.map(|v| v * dim * open));
                }
            }
        }
        EndingKind::Victory => {
            // Destello blanco calido y despues un fondo ambar con el resplandor del Sol al centro.
            let flash =
                smoothstep(0.0, 0.7, seconds) * (1.0 - smoothstep(0.9, FADE_SECONDS, seconds));
            let settle = smoothstep(0.9, FADE_SECONDS, seconds);
            let white = [1.0, 0.96, 0.86];
            for y in 0..fb.height {
                for x in 0..fb.width {
                    let dx = x as f64 + 0.5 - w * 0.5;
                    let dy = y as f64 + 0.5 - h * 0.45;
                    let r = (dx * dx + dy * dy).sqrt() / half_diag;
                    let glow = (1.0 - r * 1.4).clamp(0.0, 1.0).powi(2);
                    let amber = [0.10 + 0.55 * glow, 0.05 + 0.33 * glow, 0.02 + 0.12 * glow];
                    let p = &mut fb.pixels[y * fb.width + x];
                    let base = mix(unpack(*p), amber, settle);
                    *p = pack(mix(base, white, flash * 0.92));
                }
            }
        }
    }
}

fn wrap(text: &str, columns: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + word.len() + 1 > columns {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn text_width(text: &str, scale: usize) -> usize {
    text.chars().count() * 6 * scale
}

/// Dibuja la presentacion final completa sobre el cuadro ya compuesto.
/// `seconds`: tiempo real desde el final (`EndingClock::seconds`).
pub fn draw_ending(fb: &mut Framebuffer, summary: &EndingSummary, seconds: f64) {
    if !seconds.is_finite() || fb.width == 0 || fb.height == 0 {
        return;
    }
    let seconds = seconds.max(0.0);
    apply_transition(fb, summary.kind, seconds);
    if seconds < TEXT_DELAY {
        return;
    }
    let big = fb.width >= 960 && fb.height >= 540;
    let (title_scale, body_scale) = if big {
        (5, 2)
    } else if fb.width >= 480 {
        (3, 1)
    } else {
        (2, 1)
    };
    let (title, paragraphs, stats) = ending_text(summary);
    let columns = (fb.width * 3 / 4) / (6 * body_scale);
    let body: Vec<String> = paragraphs
        .iter()
        .flat_map(|p| {
            let mut lines = wrap(p, columns.max(8));
            lines.push(String::new());
            lines
        })
        .collect();
    let (title_color, body_color, accent) = match summary.kind {
        EndingKind::Death => (0xd8444a, 0xc9c2c4, 0x8a7f86),
        EndingKind::Victory => (0xffe2a6, 0xf6ead2, 0xd8b779),
    };
    // Revelado tipo maquina de escribir: titulo, cuerpo y estadisticas en orden.
    let mut budget = ((seconds - TEXT_DELAY) * CHARS_PER_SECOND) as usize;
    fn take(budget: &mut usize, text: &str) -> String {
        let n = text.chars().count().min(*budget);
        *budget -= n;
        text.chars().take(n).collect()
    }
    let title_h = 7 * title_scale;
    let row = 8 * body_scale + 3;
    let total_h = title_h + 3 * row + body.len() * row + 2 * row + row;
    let mut y = fb.height.saturating_sub(total_h) / 2;
    let panel_w = (fb.width * 5 / 6).min(fb.width);
    let panel_x = (fb.width - panel_w) / 2;
    let fade_in = smoothstep(TEXT_DELAY, TEXT_DELAY + 0.8, seconds);
    fb.darken_rect(
        panel_x,
        y.saturating_sub(row * 2),
        panel_w,
        total_h + row * 4,
        0.45 * fade_in,
    );
    let width = fb.width;
    let centered =
        move |text: &str, scale: usize| width.saturating_sub(text_width(text, scale)) / 2;
    let full_title_x = centered(&title, title_scale);
    let shown = take(&mut budget, &title);
    draw_text(fb, full_title_x, y, title_scale, title_color, &shown);
    y += title_h + row;
    // Separador bajo el titulo.
    let rule = text_width(&title, title_scale).min(panel_w);
    let rule_x = fb.width.saturating_sub(rule) / 2;
    if budget > 0 {
        for x in rule_x..(rule_x + rule).min(fb.width) {
            fb.put_pixel(x, y, accent);
        }
    }
    y += 2 * row;
    for line in &body {
        if !line.is_empty() {
            let x = centered(line, body_scale);
            let shown = take(&mut budget, line);
            draw_text(fb, x, y, body_scale, body_color, &shown);
        }
        y += row;
    }
    y += row;
    let x = centered(&stats, body_scale);
    let shown = take(&mut budget, &stats);
    draw_text(fb, x, y, body_scale, accent, &shown);
    y += 2 * row;
    let total_chars = title.chars().count()
        + body.iter().map(|l| l.chars().count()).sum::<usize>()
        + stats.chars().count();
    let done_at = TEXT_DELAY + total_chars as f64 / CHARS_PER_SECOND + PROMPT_DELAY;
    if seconds >= done_at && ((seconds - done_at) * 1.25).fract() < 0.7 {
        let prompt = "SHIFT+R: NUEVA PARTIDA   |   ESC: SALIR";
        let x = centered(prompt, body_scale);
        draw_text(fb, x, y, body_scale, title_color, prompt);
    }
}
