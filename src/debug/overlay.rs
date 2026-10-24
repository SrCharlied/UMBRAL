//! Overlay tecnico (UMB-403): texto de contadores dibujado directamente en el framebuffer
//! con una fuente de mapa de bits 5x7, sin usar funciones de texto de ninguna biblioteca.

use crate::renderer::{framebuffer::Framebuffer, stats::RenderStats};

const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

/// Filas de arriba abajo; el bit 4 es la columna izquierda. Minusculas se dibujan en mayuscula.
fn glyph(c: char) -> [u8; GLYPH_H] {
    match c.to_ascii_uppercase() {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0A, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '%' => [0x19, 0x19, 0x02, 0x04, 0x08, 0x13, 0x13],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '=' => [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x0C, 0x04, 0x08],
        '|' => [0x04; GLYPH_H],
        ' ' => [0; GLYPH_H],
        _ => [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F], // caracter desconocido: recuadro
    }
}

/// Dibuja `text` con la esquina superior izquierda en `(x, y)`; `scale` agranda cada punto.
/// Devuelve el ancho dibujado en pixeles. Lo que cae fuera del framebuffer se ignora.
pub fn draw_text(
    fb: &mut Framebuffer,
    x: usize,
    y: usize,
    scale: usize,
    color: u32,
    text: &str,
) -> usize {
    let advance = (GLYPH_W + 1) * scale;
    for (i, ch) in text.chars().enumerate() {
        for (row, bits) in glyph(ch).into_iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            fb.put_pixel(
                                x + i * advance + col * scale + dx,
                                y + row * scale + dy,
                                color,
                            );
                        }
                    }
                }
            }
        }
    }
    text.chars().count() * advance
}

/// Interruptores visibles en el overlay.
#[derive(Clone, Copy, Debug)]
pub struct OverlayState {
    pub culling: bool,
    pub lod: bool,
    pub backface: bool,
    pub orbits_frozen: bool,
}

/// Lineas de texto del overlay; separado del dibujo para poder probarlo.
pub fn overlay_lines(
    fps: Option<f64>,
    stats: &RenderStats,
    state: &OverlayState,
    speed: f64,
    oxygen01: f64,
    time: f64,
) -> Vec<String> {
    let onoff = |b: bool| if b { "ON" } else { "OFF" };
    let mut lines = Vec::new();
    if let Some(fps) = fps {
        lines.push(format!(
            "FPS {fps:.0}  FRAME {:.1} MS",
            1000.0 / fps.max(1e-9)
        ));
    }
    lines.extend([
        format!("TIEMPO {time:.1} S  VELOCIDAD {speed:.0}"),
        format!("OXIGENO {:.0}% (DEBUG)", oxygen01 * 100.0),
        format!(
            "OBJETOS {} DESCARTADOS {}",
            stats.objects_submitted, stats.objects_culled
        ),
        format!("VERTICES {}", stats.vertices_processed),
        format!(
            "TRIANGULOS ENVIADOS {} DESCARTADOS {} CARAS TRASERAS {}",
            stats.triangles_submitted, stats.triangles_culled, stats.triangles_backface
        ),
        format!(
            "RECORTADOS {} RASTERIZADOS {}",
            stats.triangles_clipped, stats.triangles_rasterized
        ),
        format!(
            "FRAGMENTOS {} PIXELES {}",
            stats.fragments_evaluated, stats.pixels_written
        ),
        format!(
            "F3 CULLING {}  F4 LOD {}  F6 CARAS TRASERAS {}  F5 ORBITAS {}",
            onoff(state.culling),
            onoff(state.lod),
            onoff(state.backface),
            if state.orbits_frozen {
                "CONGELADAS"
            } else {
                "LIBRES"
            }
        ),
    ]);
    lines
}

/// Dibuja el overlay sobre una banda oscura semitransparente en la esquina superior izquierda.
pub fn draw_overlay(fb: &mut Framebuffer, lines: &[String]) {
    let scale = if fb.width >= 1000 { 2 } else { 1 };
    let line_h = (GLYPH_H + 3) * scale;
    let width = lines
        .iter()
        .map(|l| l.chars().count() * (GLYPH_W + 1) * scale)
        .max()
        .unwrap_or(0)
        + 16;
    fb.darken_rect(0, 0, width, lines.len() * line_h + 12, 0.55);
    for (i, line) in lines.iter().enumerate() {
        draw_text(fb, 8, 8 + i * line_h, scale, 0xE8F0F0, line);
    }
}
