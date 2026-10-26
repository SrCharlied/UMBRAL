//! Distribucion procedural reproducible sobre la esfera unidad local de un planeta
//! (UMB-304). Detalles, unidades y limites en `docs/procedural.md`.

use crate::math::Vec3;

/// Intentos maximos por llamada; se valida antes de reservar memoria.
pub const MAX_ATTEMPTS: u32 = 4096;
/// Celdas de ruido por unidad de radio admitidas como maximo.
pub const MAX_NOISE_SCALE: f64 = 1024.0;
/// Limite de `|point * noise_scale|` por coordenada para `density` (2^20).
pub const MAX_NOISE_COORD: f64 = 1_048_576.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistributionSettings {
    pub seed: u64,
    /// Candidatos evaluados, `0..=MAX_ATTEMPTS`; cero produce una salida vacia.
    pub attempts: u32,
    /// Frecuencia del ruido: celdas por unidad de radio, `(0, MAX_NOISE_SCALE]`.
    pub noise_scale: f64,
    /// Densidad minima para aceptar un candidato, `[0, 1]`.
    pub threshold: f64,
    /// Distancia cordal minima entre muestras sobre la esfera unidad, `[0, 2]`; 0 la desactiva.
    pub min_spacing: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceSample {
    /// Punto de la esfera unidad, espacio local del planeta.
    pub position: Vec3,
    /// Normal radial unitaria (igual a `position` mientras la superficie sea la esfera).
    pub normal: Vec3,
    /// Densidad del ruido en `position`, `[0, 1]` (ver `density`).
    pub density: f64,
    /// Indice del candidato que produjo la muestra.
    pub attempt: u32,
}

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
const STREAM_SAMPLE: u64 = 1;
const STREAM_NOISE: u64 = 2;

/// Finalizador de SplitMix64: cada bit de entrada afecta a todos los de salida.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hash por contador: el valor depende solo de `(seed, stream, index)`, no del orden de uso.
fn hash(seed: u64, stream: u64, index: u64) -> u64 {
    mix(mix(seed ^ stream.wrapping_mul(GOLDEN)) ^ index)
}

/// `[0, 1)` con los 53 bits altos.
fn unit(bits: u64) -> f64 {
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

/// Candidato `i`, uniforme en area: `y` uniforme en `[-1, 1]` y longitud uniforme
/// (teorema de Arquimedes), sin acumulacion en los polos.
fn candidate(seed: u64, attempt: u32) -> Vec3 {
    let i = u64::from(attempt);
    let y = 1.0 - 2.0 * unit(hash(seed, STREAM_SAMPLE, 2 * i));
    let phi = std::f64::consts::TAU * unit(hash(seed, STREAM_SAMPLE, 2 * i + 1));
    let r = (1.0 - y * y).max(0.0).sqrt();
    let (sin, cos) = phi.sin_cos();
    Vec3::new(r * cos, y, r * sin).normalized()
}

impl DistributionSettings {
    /// Rangos en `docs/procedural.md`. Rechaza NaN e infinitos.
    fn validate(&self) -> Result<(), String> {
        let in_range = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if self.attempts > MAX_ATTEMPTS {
            return Err(format!(
                "attempts excede el presupuesto de {MAX_ATTEMPTS}: {}",
                self.attempts
            ));
        }
        check_noise_scale(self.noise_scale)?;
        if !in_range(self.threshold, 0.0, 1.0) {
            return Err(format!(
                "threshold debe ser finito y estar en [0, 1]: {}",
                self.threshold
            ));
        }
        if !in_range(self.min_spacing, 0.0, 2.0) {
            return Err(format!(
                "min_spacing (cuerda en la esfera unidad) debe ser finito y estar en [0, 2]: {}",
                self.min_spacing
            ));
        }
        Ok(())
    }
}

/// Muestras aceptadas en orden creciente de intento. Posicion y normal estan en el
/// espacio local del planeta (esfera de radio 1 centrada en el origen); no incluyen
/// orbita, rotacion ni tiempo.
pub fn distribute(settings: &DistributionSettings) -> Result<Vec<SurfaceSample>, String> {
    settings.validate()?;
    let spacing_sq = settings.min_spacing * settings.min_spacing;
    let mut samples = Vec::with_capacity(settings.attempts as usize);
    for attempt in 0..settings.attempts {
        let position = candidate(settings.seed, attempt);
        // Entradas ya validadas y punto unitario: dentro del dominio de `density`.
        let density = value_noise(settings.seed, settings.noise_scale, position);
        // Voraz en orden de intento: O(intentos * aceptadas), acotado por MAX_ATTEMPTS.
        let crowded = |s: &SurfaceSample| {
            let d = s.position - position;
            d.dot(d) < spacing_sq
        };
        if density < settings.threshold || samples.iter().any(crowded) {
            continue;
        }
        samples.push(SurfaceSample {
            position,
            normal: position,
            density,
            attempt,
        });
    }
    Ok(samples)
}

/// Valor pseudoaleatorio en `[0, 1)` asociado a un nodo entero de la reticula.
fn lattice(seed: u64, x: i64, y: i64, z: i64) -> f64 {
    let node = mix(x as u64 ^ mix(y as u64 ^ mix(z as u64)));
    unit(hash(seed, STREAM_NOISE, node))
}

fn check_noise_scale(noise_scale: f64) -> Result<(), String> {
    if noise_scale.is_finite() && noise_scale > 0.0 && noise_scale <= MAX_NOISE_SCALE {
        Ok(())
    } else {
        Err(format!(
            "noise_scale debe ser finito y estar en (0, {MAX_NOISE_SCALE}]: {noise_scale}"
        ))
    }
}

/// Densidad del campo de `distribute` en cualquier punto del dominio: `noise_scale`
/// valido, `point` finito y `|point * noise_scale| <= MAX_NOISE_COORD` por coordenada.
/// Fuera del dominio devuelve `Err`; nunca recorta ni inventa un valor. Rango `[0, 1]`:
/// los nodos valen < 1, pero la interpolacion podria redondear a 1.0 (no observado).
pub fn density(seed: u64, noise_scale: f64, point: Vec3) -> Result<f64, String> {
    check_noise_scale(noise_scale)?;
    if !point.is_finite() {
        return Err(format!("point debe ser finito: {point:?}"));
    }
    let p = point * noise_scale;
    if [p.x, p.y, p.z].iter().any(|c| c.abs() > MAX_NOISE_COORD) {
        return Err(format!(
            "point * noise_scale excede el dominio de la reticula (|coordenada| <= {MAX_NOISE_COORD}): {p:?}"
        ));
    }
    Ok(value_noise(seed, noise_scale, point))
}

/// Ruido de valor 3D: valores aleatorios en los nodos de una reticula entera, mezclados
/// por interpolacion trilineal con suavizado cubico. No es ruido de Perlin (no usa
/// gradientes). Se evalua en `point * noise_scale`, en 3D, para no tener costuras ni
/// deformacion en los polos. Sin validacion: solo para entradas dentro del dominio.
fn value_noise(seed: u64, noise_scale: f64, point: Vec3) -> f64 {
    let p = point * noise_scale;
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i64, fy as i64, fz as i64);
    let fade = |t: f64| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (fade(p.x - fx), fade(p.y - fy), fade(p.z - fz));
    let node = |dx: i64, dy: i64, dz: i64| {
        lattice(
            seed,
            ix.wrapping_add(dx),
            iy.wrapping_add(dy),
            iz.wrapping_add(dz),
        )
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let y0 = lerp(
        lerp(node(0, 0, 0), node(1, 0, 0), u),
        lerp(node(0, 1, 0), node(1, 1, 0), u),
        v,
    );
    let y1 = lerp(
        lerp(node(0, 0, 1), node(1, 0, 1), u),
        lerp(node(0, 1, 1), node(1, 1, 1), u),
        v,
    );
    lerp(y0, y1, w)
}
