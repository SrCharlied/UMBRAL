//! Marcadores de navegacion en pantalla (experiencia de presentacion): rombo cian con «O2» y
//! distancia para los tanques de oxigeno mas cercanos, y rombo dorado con «REG» para los
//! registros que faltan. Si el objetivo queda fuera de vista (o detras de la camara), el
//! marcador se pega al borde de la pantalla en su direccion.

use crate::{
    debug::overlay::draw_text, math::Vec3, renderer::framebuffer::Framebuffer, scene::world::World,
};

/// Tanques marcados a la vez (los mas cercanos).
pub const MARKED_TANKS: usize = 3;
/// Distancia maxima para marcar un tanque.
pub const TANK_MARK_DISTANCE: f64 = 1600.0;

const TANK_COLOR: u32 = 0x3ff5df;
const LOG_COLOR: u32 = 0xffc040;

/// Objetivos que se marcan: `(posicion, es_registro)`.
pub fn marker_targets(world: &World) -> Vec<(Vec3, bool)> {
    let ship = world.ship.position;
    let mut tanks: Vec<Vec3> = world
        .gameplay
        .resources
        .iter()
        .filter(|r| !r.collected)
        .filter_map(|r| r.position(&world.system, world.time))
        .chain(
            world
                .sectors
                .iter()
                .flat_map(|field| field.tanks().map(|t| t.position)),
        )
        .filter(|p| (*p - ship).length() <= TANK_MARK_DISTANCE)
        .collect();
    tanks.sort_by(|a, b| (*a - ship).length().total_cmp(&(*b - ship).length()));
    tanks.truncate(MARKED_TANKS);
    let logs = world
        .gameplay
        .logs
        .iter()
        .filter(|l| !l.anchor.collected)
        .filter_map(|l| l.anchor.position(&world.system, world.time));
    tanks
        .into_iter()
        .map(|p| (p, false))
        .chain(logs.map(|p| (p, true)))
        .collect()
}

/// Dibuja los marcadores sobre el cuadro ya compuesto.
pub fn draw_markers(fb: &mut Framebuffer, world: &World) {
    if world.gameplay.game_over || world.gameplay.objective_complete {
        return;
    }
    let (w, h) = (fb.width as f64, fb.height as f64);
    if w < 32.0 || h < 32.0 {
        return;
    }
    let view_projection = world.camera.view_projection();
    for (position, is_log) in marker_targets(world) {
        let clip = view_projection.transform(position.extend(1.0));
        if ![clip.x, clip.y, clip.w].iter().all(|v| v.is_finite()) {
            continue;
        }
        let behind = clip.w <= 1e-6;
        let (mut nx, mut ny) = if behind {
            (-clip.x, -clip.y)
        } else {
            (clip.x / clip.w, clip.y / clip.w)
        };
        let edge = 0.9;
        let off = behind || nx.abs() > 1.0 || ny.abs() > 1.0;
        if off {
            let m = nx.abs().max(ny.abs()).max(1e-9);
            nx = nx / m * edge;
            ny = ny / m * edge;
        }
        let x = ((nx * 0.5 + 0.5) * w) as isize;
        let y = ((0.5 - ny * 0.5) * h) as isize;
        let color = if is_log { LOG_COLOR } else { TANK_COLOR };
        let size = if off { 6 } else { 9 };
        diamond(fb, x, y, size, color);
        let distance = (position - world.ship.position).length();
        let label = format!("{} {:.0}", if is_log { "REG" } else { "O2" }, distance);
        let tx = (x + size + 4).clamp(0, w as isize - 6 * label.len() as isize - 2);
        let ty = (y - 3).clamp(0, h as isize - 9);
        draw_text(fb, tx as usize, ty as usize, 1, color, &label);
    }
}

/// Contorno de rombo de radio `r` centrado en `(cx, cy)`, doble trazo.
fn diamond(fb: &mut Framebuffer, cx: isize, cy: isize, r: isize, color: u32) {
    for k in 0..=r {
        for (dx, dy) in [(k, r - k), (-k, r - k), (k, k - r), (-k, k - r)] {
            for t in 0..2 {
                let (x, y) = (cx + dx, cy + dy + t);
                if x >= 0 && y >= 0 {
                    fb.put_pixel(x as usize, y as usize, color);
                }
            }
        }
    }
}
