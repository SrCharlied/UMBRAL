//! UMB-310 variante 02 (preview AISLADO): grises + morado del monstruo, tunel hacia la nave,
//! vision borrosa y negro total con O2 0 o con la muestra explicita de ataque.
use umbral::renderer::framebuffer::{Framebuffer, MAX_PIXELS};

/// Con este O2 o mas (y sin ataque) la imagen es identica a la escena.
pub const PERCEPTION_ONSET_OXYGEN: f64 = 0.5;
/// Con este O2 o menos tinte, tunel y desenfoque estan al maximo (antes del negro final).
pub const PERCEPTION_FULL_OXYGEN: f64 = 0.05;
/// Referencias de tono medidas por Veliona en la textura del monstruo (solo numeros).
pub const MONSTER_PURPLE: [f64; 3] = [65.0, 20.0, 67.0];
pub const MONSTER_GREY: [f64; 3] = [125.0, 117.0, 119.0];
/// Peso del morado frente al gris en el tinte (el resto es gris).
pub const TINT_PURPLE_SHARE: f64 = 0.6;
/// Mezcla maxima hacia el gris-violeta (el resto conserva el color original).
pub const TINT_MAX: f64 = 0.9;

/// Tunel (radios en unidades del lado menor, medidos desde el foco): abierto mas alla de la
/// esquina mas lejana al empezar, cerrado alrededor de la nave en el tramo critico.
pub const TUNNEL_OPEN_RADIUS: f64 = 2.2;
pub const TUNNEL_OPEN_SOFTNESS: f64 = 0.5;
pub const TUNNEL_CLOSED_RADIUS: f64 = 0.19;
pub const TUNNEL_CLOSED_SOFTNESS: f64 = 0.11;

/// Ultimo tramo: entre este O2 y 0 todo funde a negro (al 1 % la nave aun se ve completa).
pub const BLACKOUT_FADE_OXYGEN: f64 = 0.01;
/// Desenfoque gaussiano maximo: sigma como fraccion del lado menor (3.6 px a 1280x720).
pub const BLUR_MAX_SIGMA_FRACTION: f64 = 0.005;

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn luma(c: [f64; 3]) -> f64 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Color multiplicador de luminancia 1: mezcla del gris y el morado de referencia, cada uno
/// normalizado por su luminancia. `luma * tinte` conserva el brillo y deja el negro en negro.
fn tint_color() -> [f64; 3] {
    let (purple, grey) = (luma(MONSTER_PURPLE), luma(MONSTER_GREY));
    std::array::from_fn(|i| {
        TINT_PURPLE_SHARE * MONSTER_PURPLE[i] / purple
            + (1.0 - TINT_PURPLE_SHARE) * MONSTER_GREY[i] / grey
    })
}

fn channels(p: u32) -> [f64; 3] {
    [
        ((p >> 16) & 0xff) as f64,
        ((p >> 8) & 0xff) as f64,
        (p & 0xff) as f64,
    ]
}

fn pack(c: [f64; 3]) -> u32 {
    let ch = |v: f64| v.round().clamp(0.0, 255.0) as u32;
    (ch(c[0]) << 16) | (ch(c[1]) << 8) | ch(c[2])
}

/// Nucleo gaussiano normalizado (radio `ceil(3 sigma)`; `[1]` sin desenfoque) y primera pasada
/// horizontal del desenfoque separable, leyendo solo de `source` y con bordes acotados.
fn blur_rows(source: &Framebuffer, sigma: f64) -> (Vec<f64>, Vec<[f32; 3]>) {
    let radius = (3.0 * sigma).ceil() as usize;
    let mut kernel: Vec<f64> = (0..=2 * radius)
        .map(|k| {
            let d = k as f64 - radius as f64;
            if radius == 0 {
                1.0
            } else {
                (-d * d / (2.0 * sigma * sigma)).exp()
            }
        })
        .collect();
    let total: f64 = kernel.iter().sum();
    kernel.iter_mut().for_each(|w| *w /= total);
    let width = source.width;
    let mut rows = Vec::with_capacity(source.pixels.len());
    for line in source.pixels.chunks_exact(width) {
        for x in 0..width {
            let mut c = [0.0; 3];
            for (k, weight) in kernel.iter().enumerate() {
                let v = channels(line[(x + k).saturating_sub(radius).min(width - 1)]);
                for i in 0..3 {
                    c[i] += weight * v[i];
                }
            }
            rows.push(c.map(|v| v as f32));
        }
    }
    (kernel, rows)
}

/// Rechaza entradas sin sentido ANTES de reservar o escribir, tambien en los atajos de
/// identidad y de negro: O2 fuera de `[0, 1]`, tiempo negativo o no finito, foco fuera de la
/// pantalla `[0, 1]^2` o no finito, y un `Framebuffer` con dimensiones publicas incongruentes con
/// `pixels`, nulas, que desbordan o exceden `MAX_PIXELS`. No sanea a un valor valido.
fn validate(
    source: &Framebuffer,
    oxygen01: f64,
    elapsed_time: f64,
    focus_uv: [f64; 2],
) -> Result<(), String> {
    if !(0.0..=1.0).contains(&oxygen01) {
        return Err(format!("O2 de percepcion fuera de [0, 1]: {oxygen01}"));
    }
    if !(elapsed_time.is_finite() && elapsed_time >= 0.0) {
        return Err(format!(
            "tiempo de percepcion invalido (segundos >= 0): {elapsed_time}"
        ));
    }
    if !focus_uv.iter().all(|v| (0.0..=1.0).contains(v)) {
        return Err(format!("foco fuera de la pantalla [0, 1]: {focus_uv:?}"));
    }
    let (width, height) = (source.width, source.height);
    match width.checked_mul(height) {
        Some(count) if count > 0 && count <= MAX_PIXELS && count == source.pixels.len() => Ok(()),
        _ => Err(format!(
            "framebuffer incongruente: {width}x{height} con {} pixeles (maximo {MAX_PIXELS})",
            source.pixels.len()
        )),
    }
}

pub fn lucid_perception_variant(
    source: &Framebuffer,
    oxygen01: f64,
    elapsed_time: f64,
    focus_uv: [f64; 2],
    attack_blackout: bool,
) -> Result<Vec<u32>, String> {
    validate(source, oxygen01, elapsed_time, focus_uv)?;
    if attack_blackout || oxygen01 == 0.0 {
        return Ok(vec![0; source.pixels.len()]);
    }
    let strength = smoothstep(PERCEPTION_ONSET_OXYGEN, PERCEPTION_FULL_OXYGEN, oxygen01);
    if strength == 0.0 {
        return Ok(source.pixels.clone());
    }
    let (width, height) = (source.width, source.height);
    let (tint, amount) = (tint_color(), TINT_MAX * strength);
    let short = width.min(height) as f64;
    let (fx, fy) = (focus_uv[0] * width as f64, focus_uv[1] * height as f64);
    let inner = lerp(TUNNEL_OPEN_RADIUS, TUNNEL_CLOSED_RADIUS, strength);
    let outer = inner + lerp(TUNNEL_OPEN_SOFTNESS, TUNNEL_CLOSED_SOFTNESS, strength);
    let fade = smoothstep(0.0, BLACKOUT_FADE_OXYGEN, oxygen01);
    let (kernel, blurred_rows) = blur_rows(source, BLUR_MAX_SIGMA_FRACTION * short * strength);
    let radius = kernel.len() / 2;
    let mut output = Vec::with_capacity(source.pixels.len());
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = (x as f64 + 0.5 - fx, y as f64 + 0.5 - fy);
            let visible = 1.0 - smoothstep(inner, outer, (dx * dx + dy * dy).sqrt() / short);
            if visible == 0.0 {
                output.push(0);
                continue;
            }
            // Segunda pasada (vertical) solo donde el tunel deja ver.
            let mut c = [0.0; 3];
            for (k, weight) in kernel.iter().enumerate() {
                let row = (y + k).saturating_sub(radius).min(height - 1);
                let v = blurred_rows[row * width + x];
                for i in 0..3 {
                    c[i] += weight * v[i] as f64;
                }
            }
            let l = luma(c);
            let pixel = pack(std::array::from_fn(|i| {
                (c[i] + (l * tint[i] - c[i]) * amount) * (visible * fade)
            }));
            // Solo en el fundido final: no perder toda la senal por redondeo a RGB8.
            // Con luminancia positiva el tinte tiene tres canales positivos; el piso
            // de un nivel no ilumina una muestra negra ni el exterior del tunel.
            output.push(if fade < 1.0 && pixel == 0 && l > 0.0 {
                0x010101
            } else {
                pixel
            });
        }
    }
    Ok(output)
}
