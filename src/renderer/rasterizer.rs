use super::framebuffer::Framebuffer;

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
