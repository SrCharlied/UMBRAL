//! Menu de inicio (al estilo de DREDGE, pedido de Charlie): sobre la escena REAL del juego (el Sol, los planetas y la
//! nave, sin avanzar la partida) se oscurece el lado izquierdo y van solo el titulo UMBRAL y dos botones oscuros,
//! JUGAR y CONTROLES. Esc sale del juego desde el menu y vuelve desde CONTROLES. Logica pura (`StartMenu::step`)
//! separada del dibujo (`draw_start_menu`) para probarla sin ventana; la app solo lee teclas y presenta.

use crate::{debug::overlay::draw_text, renderer::framebuffer::Framebuffer};

pub const MENU_ITEMS: [&str; 2] = ["JUGAR", "CONTROLES"];
pub const TITLE: &str = "UMBRAL";
/// Hueso claro del titulo y del texto; ambar del boton elegido.
pub const TITLE_COLOR: u32 = 0xece4d6;
pub const SELECTED_TEXT: u32 = 0xffd08a;
const TEXT: u32 = 0xe8e2d6;
const DIM_TEXT: u32 = 0x9a9284;
const ACCENT: u32 = 0xd8a25a;
/// Bordes cromaticos del titulo (cian y rojo, como el de DREDGE).
const TITLE_CYAN: u32 = 0x3aa6a0;
const TITLE_RED: u32 = 0xb8433a;
const BUTTON: u32 = 0x07080b;
const BUTTON_SELECTED: u32 = 0x1e1510;

const CONTROLS: [&str; 8] = [
    "W / S: EMPUJE | A / D: GIRO",
    "FLECHAS: CABECEO | Q / E: ALABEO",
    "SHIFT: IMPULSO | O: ASISTENCIA",
    "N / M Y 1-4: OXIGENO | L: RITMO",
    "P: CAPTURA | SHIFT+R: REINICIAR",
    "ESC: SALIR",
    "OBJETIVO: LEE LOS TRES REGISTROS",
    "LOS TANQUES CIAN TE DAN OXIGENO",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuScreen {
    #[default]
    Main,
    Controls,
}

/// Teclas de UN cuadro (pulsaciones, sin repeticion).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub confirm: bool,
    pub back: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Stay,
    StartGame,
    Quit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartMenu {
    pub selected: usize,
    pub screen: MenuScreen,
}

impl StartMenu {
    pub fn new() -> Self {
        Self::default()
    }

    /// Un cuadro de teclas. En el menu principal: arriba/abajo con vuelta, Enter elige y Esc sale. En CONTROLES,
    /// Enter o Esc vuelven al menu principal con la misma seleccion.
    pub fn step(&mut self, input: MenuInput) -> MenuAction {
        let count = MENU_ITEMS.len();
        match self.screen {
            MenuScreen::Main => {
                if input.back {
                    return MenuAction::Quit;
                }
                if input.up {
                    self.selected = (self.selected + count - 1) % count;
                }
                if input.down {
                    self.selected = (self.selected + 1) % count;
                }
                if input.confirm {
                    if self.selected == 0 {
                        return MenuAction::StartGame;
                    }
                    self.screen = MenuScreen::Controls;
                }
            }
            MenuScreen::Controls => {
                if input.back || input.confirm {
                    self.screen = MenuScreen::Main;
                }
            }
        }
        MenuAction::Stay
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// Posiciones del menu para un tamano de cuadro.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuLayout {
    /// Escala de la fuente 5x7 del titulo y del resto del texto.
    pub title_scale: usize,
    pub title: MenuRect,
    pub text_scale: usize,
    pub items: [MenuRect; 2],
    /// Panel de CONTROLES (en el lugar de los botones).
    pub panel: MenuRect,
}

fn text_width(text: &str, scale: usize) -> usize {
    (text.chars().count() * 6).saturating_sub(1) * scale
}

pub fn menu_layout(width: usize, height: usize) -> MenuLayout {
    let s = (height / 300).max(1);
    let title_cols = TITLE.len() * 6 - 1;
    let ts = (height / 52)
        .max(3)
        .min((width * 55 / 100) / title_cols)
        .max(1);
    let offset = (ts / 5).max(1);
    let title = MenuRect {
        x: width * 7 / 100,
        y: height * 18 / 100,
        w: title_cols * ts + 2 * offset,
        h: 7 * ts,
    };
    let widest = MENU_ITEMS
        .iter()
        .map(|t| text_width(t, s))
        .max()
        .unwrap_or(0);
    let (bw, bh, gap) = (widest + 40 * s, 19 * s, 5 * s);
    let x = title.x + ts;
    let y0 = title.y + title.h + 14 * s;
    let items = std::array::from_fn(|i| MenuRect {
        x,
        y: y0 + i * (bh + gap),
        w: bw,
        h: bh,
    });
    let available = width.saturating_sub(x + 8);
    let panel = MenuRect {
        x,
        y: y0,
        w: (240 * s).min(available).max(1),
        // Encabezado, las lineas de controles y el pie VOLVER.
        h: (146 * s).min(height.saturating_sub(y0 + 1)).max(1),
    };
    MenuLayout {
        title_scale: ts,
        title,
        text_scale: s,
        items,
        panel,
    }
}

/// Mezcla `color` sobre el rectangulo (recortado al cuadro) con opacidad `alpha`.
fn fill(fb: &mut Framebuffer, r: MenuRect, color: u32, alpha: f64) {
    let ch = |c: u32, shift: u32| f64::from((c >> shift) & 0xff);
    for y in r.y..(r.y + r.h).min(fb.height) {
        for x in r.x..(r.x + r.w).min(fb.width) {
            let p = &mut fb.pixels[y * fb.width + x];
            let mix = |shift: u32| {
                let v = ch(*p, shift) + (ch(color, shift) - ch(*p, shift)) * alpha;
                (v.round().clamp(0.0, 255.0) as u32) << shift
            };
            *p = mix(16) | mix(8) | mix(0);
        }
    }
}

/// Parte `text` en lineas de hasta `columns` caracteres por palabras.
fn wrap(text: &str, columns: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().expect("siempre hay una linea");
        if !line.is_empty() && line.len() + 1 + word.len() > columns {
            lines.push(word.to_string());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

/// Dibuja el menu sobre el cuadro ya renderizado de la escena.
pub fn draw_start_menu(fb: &mut Framebuffer, menu: &StartMenu) {
    let (w, h) = (fb.width, fb.height);
    if w < 8 || h < 8 {
        return;
    }
    let l = menu_layout(w, h);
    let (s, ts) = (l.text_scale, l.title_scale);
    // Degradado oscuro a la izquierda: el titulo y los botones se leen y la escena sigue a la derecha.
    let gradient = (w * 55 / 100).max(1);
    for x in 0..gradient {
        let t = 1.0 - x as f64 / gradient as f64;
        fb.darken_rect(x, 0, 1, h, 0.62 * t.powf(1.5));
    }
    // Titulo con bordes cromaticos.
    let offset = (ts / 5).max(1);
    let (tx, ty) = (l.title.x, l.title.y);
    draw_text(fb, tx, ty, ts, TITLE_CYAN, TITLE);
    draw_text(fb, tx + 2 * offset, ty, ts, TITLE_RED, TITLE);
    draw_text(fb, tx + offset, ty, ts, TITLE_COLOR, TITLE);
    match menu.screen {
        MenuScreen::Main => {
            for (i, (label, r)) in MENU_ITEMS.iter().zip(l.items).enumerate() {
                let selected = i == menu.selected;
                fill(fb, r, if selected { BUTTON_SELECTED } else { BUTTON }, 0.85);
                if selected {
                    fill(fb, MenuRect { w: 2 * s, ..r }, ACCENT, 1.0);
                }
                let color = if selected { SELECTED_TEXT } else { TEXT };
                let lx = r.x + r.w.saturating_sub(text_width(label, s)) / 2;
                draw_text(fb, lx, r.y + (r.h - 7 * s) / 2, s, color, label);
            }
        }
        MenuScreen::Controls => {
            let p = l.panel;
            fill(fb, p, BUTTON, 0.86);
            fill(fb, MenuRect { w: 2 * s, ..p }, ACCENT, 1.0);
            let pad = 8 * s;
            let columns = (p.w.saturating_sub(2 * pad) / (6 * s)).max(8);
            let mut y = p.y + pad;
            draw_text(fb, p.x + pad, y, s, SELECTED_TEXT, "CONTROLES");
            y += 7 * s + 6 * s;
            for line in CONTROLS.iter().flat_map(|t| wrap(t, columns)) {
                if y + 7 * s + 7 * s + pad > p.y + p.h {
                    break;
                }
                draw_text(fb, p.x + pad, y, s, TEXT, &line);
                y += 7 * s + 4 * s;
            }
            if p.y + p.h > 9 * s + pad {
                let back = "ENTER O ESC: VOLVER";
                let back_y = p.y + p.h - 7 * s - pad / 2;
                draw_text(fb, p.x + pad, back_y, s, DIM_TEXT, back);
            }
        }
    }
}
