use super::framebuffer::Framebuffer;
use super::shader::{Color, FragmentInput, to_rgb8};
use crate::math::{Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct ScreenVertex {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

fn edge(a: ScreenVertex, b: ScreenVertex, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

pub fn triangle(framebuffer: &mut Framebuffer, mut vertices: [ScreenVertex; 3]) -> usize {
    if vertices
        .iter()
        .any(|v| !v.x.is_finite() || !v.y.is_finite() || !v.z.is_finite())
    {
        return 0;
    }
    let area = edge(vertices[0], vertices[1], vertices[2].x, vertices[2].y);
    if !area.is_finite() || area.abs() < 1e-12 {
        return 0;
    }
    if area < 0.0 {
        vertices.swap(1, 2);
    }
    let area = area.abs();
    let [a, b, c] = vertices;
    let x_min = a.x.min(b.x).min(c.x).floor().max(0.0) as isize;
    let x_max =
        a.x.max(b.x)
            .max(c.x)
            .ceil()
            .min(framebuffer.width as f64 - 1.0) as isize;
    let y_min = a.y.min(b.y).min(c.y).floor().max(0.0) as isize;
    let y_max =
        a.y.max(b.y)
            .max(c.y)
            .ceil()
            .min(framebuffer.height as f64 - 1.0) as isize;
    let mut written = 0;
    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let w0 = edge(b, c, px, py) / area;
            let w1 = edge(c, a, px, py) / area;
            let w2 = edge(a, b, px, py) / area;
            if w0 >= -1e-10 && w1 >= -1e-10 && w2 >= -1e-10 {
                let depth = w0 * a.z + w1 * b.z + w2 * c.z;
                written += usize::from(framebuffer.point(x, y, depth));
            }
        }
    }
    written
}

/// Vertice ya proyectado a pantalla con los atributos que se interpolan.
#[derive(Clone, Copy, Debug)]
pub struct RasterVertex {
    /// Pixeles y profundidad NDC (lineal en pantalla).
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// `1 / w` de clip, base de la correccion de perspectiva.
    pub inverse_w: f64,
    pub world_position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

/// Rasteriza un triangulo ejecutando `shade` una vez por fragmento que pasa el z-test.
/// Devuelve `(fragmentos evaluados, pixeles escritos)`.
pub fn triangle_shaded(
    framebuffer: &mut Framebuffer,
    mut v: [RasterVertex; 3],
    shade: &mut impl FnMut(&FragmentInput) -> Color,
) -> (usize, usize) {
    let finite = |p: &RasterVertex| {
        p.x.is_finite() && p.y.is_finite() && p.z.is_finite() && p.inverse_w.is_finite()
    };
    if !v.iter().all(finite) {
        return (0, 0);
    }
    let edge_of = |a: &RasterVertex, b: &RasterVertex, x: f64, y: f64| {
        (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
    };
    let mut area = edge_of(&v[0], &v[1], v[2].x, v[2].y);
    if !area.is_finite() || area.abs() < 1e-12 {
        return (0, 0);
    }
    if area < 0.0 {
        v.swap(1, 2);
        area = -area;
    }
    let [a, b, c] = v;
    let x_min = a.x.min(b.x).min(c.x).floor().max(0.0) as isize;
    let x_max =
        a.x.max(b.x)
            .max(c.x)
            .ceil()
            .min(framebuffer.width as f64 - 1.0) as isize;
    let y_min = a.y.min(b.y).min(c.y).floor().max(0.0) as isize;
    let y_max =
        a.y.max(b.y)
            .max(c.y)
            .ceil()
            .min(framebuffer.height as f64 - 1.0) as isize;
    let (mut fragments, mut written) = (0, 0);
    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let w0 = edge_of(&b, &c, px, py) / area;
            let w1 = edge_of(&c, &a, px, py) / area;
            let w2 = edge_of(&a, &b, px, py) / area;
            if w0 < -1e-10 || w1 < -1e-10 || w2 < -1e-10 {
                continue;
            }
            let depth = w0 * a.z + w1 * b.z + w2 * c.z;
            if !framebuffer.depth_passes(x, y, depth) {
                continue;
            }
            // Con correccion de perspectiva: cada atributo se pondera por 1/w.
            let (p0, p1, p2) = (w0 * a.inverse_w, w1 * b.inverse_w, w2 * c.inverse_w);
            let inv = 1.0 / (p0 + p1 + p2);
            let (p0, p1, p2) = (p0 * inv, p1 * inv, p2 * inv);
            let fragment = FragmentInput {
                screen_position: Vec3::new(px, py, depth),
                world_position: a.world_position * p0
                    + b.world_position * p1
                    + c.world_position * p2,
                normal: a.normal * p0 + b.normal * p1 + c.normal * p2,
                uv: a.uv * p0 + b.uv * p1 + c.uv * p2,
            };
            fragments += 1;
            let color = to_rgb8(shade(&fragment));
            written += usize::from(framebuffer.write(x, y, depth, color));
        }
    }
    (fragments, written)
}
