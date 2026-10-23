//! Visor heredado de `carga-modelos-24531` (commit 51fbaed). Baseline, no pipeline final:
//! vista ortografica fija, autoencuadre por modelo, color plano por cara y `clear()` en
//! cada llamada. Su sustituto es `draw_model` con MVP (UMB-101/UMB-102).

use crate::{assets::obj::Model, renderer::framebuffer::Framebuffer};

pub const BACKGROUND: u32 = 0x10191f;

pub struct RenderStats {
    pub faces_processed: usize,
    pub pixels_written: usize,
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalized(v: [f64; 3]) -> [f64; 3] {
    let length = dot(v, v).sqrt();
    if length > 0.0 {
        [v[0] / length, v[1] / length, v[2] / length]
    } else {
        [0.0; 3]
    }
}
fn channel(value: f64) -> u32 {
    let linear = value.clamp(0.0, 1.0);
    let srgb = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u32
}

pub fn render_model(
    framebuffer: &mut Framebuffer,
    model: &Model,
    angle: f64,
) -> Result<RenderStats, String> {
    use super::rasterizer::{ScreenVertex, triangle};
    if !angle.is_finite()
        || model.vertices.is_empty()
        || model.faces.is_empty()
        || model.vertices.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("Modelo o angulo no valido".into());
    }
    if model
        .faces
        .iter()
        .any(|f| f.vertex_indices.iter().any(|&i| i >= model.vertices.len()))
    {
        return Err("La cara referencia un vertice inexistente".into());
    }
    let (sin, cos) = angle.sin_cos();
    let rotated: Vec<_> = model
        .vertices
        .iter()
        .map(|v| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]])
        .collect();
    let forward = normalized([1.25, 1.15, 1.0]);
    let right = normalized(cross([0.0, 1.0, 0.0], forward));
    let up = cross(forward, right);
    let mut screen: Vec<_> = rotated
        .iter()
        .map(|&v| ScreenVertex {
            x: dot(v, right),
            y: dot(v, up),
            z: -dot(v, forward),
        })
        .collect();
    let min_x = screen.iter().map(|v| v.x).fold(f64::INFINITY, f64::min);
    let max_x = screen.iter().map(|v| v.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = screen.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
    let max_y = screen.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max);
    let (dx, dy) = (max_x - min_x, max_y - min_y);
    if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
        return Err("El modelo no tiene extension proyectable".into());
    }
    let scale = (framebuffer.width as f64 * 0.82 / dx).min(framebuffer.height as f64 * 0.82 / dy);
    if !scale.is_finite() || scale <= 0.0 {
        return Err("Escala de proyeccion invalida".into());
    }
    let (center_x, center_y) = (min_x + dx * 0.5, min_y + dy * 0.5);
    for vertex in &mut screen {
        vertex.x = (vertex.x - center_x) * scale + framebuffer.width as f64 * 0.5;
        vertex.y = framebuffer.height as f64 * 0.5 - (vertex.y - center_y) * scale;
    }
    framebuffer.clear();
    let light = normalized([0.4, 1.0, 0.65]);
    let mut stats = RenderStats {
        faces_processed: 0,
        pixels_written: 0,
    };
    for face in &model.faces {
        // OBJ guarda indices; cada cara obtiene sus tres vertices de la lista global.
        let [i0, i1, i2] = face.vertex_indices;
        let a = screen[i0];
        let b = screen[i1];
        let c = screen[i2];
        let normal = normalized(cross(
            sub(rotated[i1], rotated[i0]),
            sub(rotated[i2], rotated[i0]),
        ));
        let illumination = 0.35 + 0.65 * dot(normal, light).max(0.0);
        let diffuse = face
            .material
            .as_ref()
            .and_then(|name| model.materials.get(name))
            .copied()
            .unwrap_or([0.7, 0.6, 0.4]);
        let color = (channel(diffuse[0] * illumination) << 16)
            | (channel(diffuse[1] * illumination) << 8)
            | channel(diffuse[2] * illumination);
        framebuffer.set_color(color);
        stats.pixels_written += triangle(framebuffer, [a, b, c]);
        stats.faces_processed += 1;
    }
    Ok(stats)
}
