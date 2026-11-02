//! UMB-310 percepcion alterada por oxigeno (preview AISLADO, primera hipotesis).
use umbral::renderer::framebuffer::{Framebuffer, MAX_PIXELS};

/// Por encima de este O2 la imagen es identica (alineado con el inicio de las grietas lucidas).
pub const PERCEPTION_ONSET_OXYGEN: f64 = 0.5;
/// Con este O2 o menos el oscurecimiento periferico es maximo.
pub const PERCEPTION_FULL_OXYGEN: f64 = 0.05;
/// Bajo este O2 empieza la deformacion critica (0 justo en el umbral).
pub const DEFORM_ONSET_OXYGEN: f64 = 0.3;
/// Periodo de la onda de deformacion, en segundos simulados.
pub const DEFORM_PERIOD_SECONDS: f64 = 2.5;
/// Desplazamiento maximo de la deformacion, como fraccion del lado menor (7.2 px a 1280x720).
pub const DEFORM_MAX_FRACTION: f64 = 0.01;
/// Lobulos de la onda alrededor del centro (entero: continua en el corte de `atan2`).
pub const DEFORM_LOBES: f64 = 3.0;
/// Radio normalizado (esquinas = 1) dentro del cual la imagen queda intacta.
pub const CLEAR_RADIUS: f64 = 0.4;
/// Fraccion maxima de oscurecimiento en las esquinas.
pub const VIGNETTE_MAX: f64 = 0.7;

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rechaza entradas que no tienen sentido **antes** de reservar o escribir nada, incluido el
/// atajo de identidad: O2 fuera de `[0, 1]` o no finito, tiempo negativo o no finito (mismas
/// reglas que `--time` y `--debug-oxygen` del juego) y un `Framebuffer` cuyas dimensiones
/// publicas no cuadran con `pixels` o exceden `MAX_PIXELS`. No se sanea a un valor valido.
fn validate(source: &Framebuffer, oxygen01: f64, elapsed_time: f64) -> Result<(), String> {
    if !(0.0..=1.0).contains(&oxygen01) {
        return Err(format!("O2 de percepcion fuera de [0, 1]: {oxygen01}"));
    }
    if !(elapsed_time.is_finite() && elapsed_time >= 0.0) {
        return Err(format!(
            "tiempo de percepcion invalido (segundos >= 0): {elapsed_time}"
        ));
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

/// Postproceso RGB de un cuadro ya dibujado: devuelve colores **nuevos** y nunca toca `source`
/// (pixeles, dimensiones ni z-buffer). Con O2 >= `PERCEPTION_ONSET_OXYGEN` devuelve una copia
/// exacta. Debajo oscurece la periferia (fuera de `CLEAR_RADIUS`) de forma continua y, solo bajo
/// `DEFORM_ONSET_OXYGEN`, la deforma con una onda radial determinista que gira con el tiempo
/// simulado, muestreando con interpolacion bilineal desde `source` (coordenadas acotadas).
pub fn lucid_perception(
    source: &Framebuffer,
    oxygen01: f64,
    elapsed_time: f64,
) -> Result<Vec<u32>, String> {
    validate(source, oxygen01, elapsed_time)?;
    // 0 con O2 alto, 1 en el tramo critico; suave (derivada nula) en ambos extremos.
    let strength = smoothstep(PERCEPTION_ONSET_OXYGEN, PERCEPTION_FULL_OXYGEN, oxygen01);
    if strength == 0.0 {
        return Ok(source.pixels.clone());
    }
    let deform = smoothstep(DEFORM_ONSET_OXYGEN, PERCEPTION_FULL_OXYGEN, oxygen01);
    let (width, height) = (source.width, source.height);
    let (half_w, half_h) = (width as f64 / 2.0, height as f64 / 2.0);
    let amplitude = DEFORM_MAX_FRACTION * width.min(height) as f64 * deform;
    // Se reduce antes de multiplicar para evitar overflow; no recupera precision ya perdida.
    let phase = std::f64::consts::TAU * (elapsed_time / DEFORM_PERIOD_SECONDS).rem_euclid(1.0);
    let mut output = Vec::with_capacity(source.pixels.len());
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let (px, py) = (x as f64 + 0.5 - half_w, y as f64 + 0.5 - half_h);
            let (dx, dy) = (px / half_w, py / half_h);
            let weight = smoothstep(CLEAR_RADIUS, 1.0, ((dx * dx + dy * dy) / 2.0).sqrt());
            if weight == 0.0 {
                output.push(source.pixels[index]);
                continue;
            }
            let color = if amplitude > 0.0 {
                // `weight > 0` implica estar fuera del centro: la direccion radial existe.
                let length = (px * px + py * py).sqrt();
                let shift = amplitude * weight * (DEFORM_LOBES * dy.atan2(dx) + phase).sin();
                bilinear(
                    source,
                    x as f64 + px / length * shift,
                    y as f64 + py / length * shift,
                )
            } else {
                channels(source.pixels[index])
            };
            output.push(pack(color, 1.0 - VIGNETTE_MAX * strength * weight));
        }
    }
    Ok(output)
}

fn channels(p: u32) -> [f64; 3] {
    [
        ((p >> 16) & 0xff) as f64,
        ((p >> 8) & 0xff) as f64,
        (p & 0xff) as f64,
    ]
}

/// Escala los canales por `keep` y redondea una sola vez.
fn pack(color: [f64; 3], keep: f64) -> u32 {
    let ch = |c: f64| (c * keep).round().clamp(0.0, 255.0) as u32;
    (ch(color[0]) << 16) | (ch(color[1]) << 8) | ch(color[2])
}

/// Muestra bilineal en coordenadas de pixel, acotadas a la imagen (1x1, 1xN y Nx1 incluidos).
fn bilinear(source: &Framebuffer, x: f64, y: f64) -> [f64; 3] {
    let (max_x, max_y) = ((source.width - 1) as f64, (source.height - 1) as f64);
    let (x, y) = (x.clamp(0.0, max_x), y.clamp(0.0, max_y));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = (
        (x0 + 1).min(source.width - 1),
        (y0 + 1).min(source.height - 1),
    );
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let at = |x: usize, y: usize| channels(source.pixels[y * source.width + x]);
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    std::array::from_fn(|i| {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        top + (bottom - top) * fy
    })
}
