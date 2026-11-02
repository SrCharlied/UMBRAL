//! Punto de extension de shaders de fragmentos (UMB-105). Propietario: Persona 1;
//! las implementaciones concretas son de Persona 3 (ver `docs/architecture.md`).
//!
//! Los colores son lineales en `0..=1` (pueden excederlo para emision; se recortan al
//! convertir a sRGB 8 bits).

use crate::math::{Vec2, Vec3};
use std::f64::consts::{PI, TAU};
use std::sync::OnceLock;

pub type Color = Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderKind {
    Flat,
    LitPlanet,
    AnimatedSun,
    LucidSurface,
    VoidEye,
}

#[derive(Clone, Copy, Debug)]
pub struct FragmentInput {
    /// Pixel (x, y) y profundidad NDC `[-1, 1]`.
    pub screen_position: Vec3,
    pub world_position: Vec3,
    /// Interpolada con correccion de perspectiva; el shader la renormaliza.
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct ShaderContext {
    pub elapsed_time: f64,
    /// `0.0..=1.0`; lo produce gameplay (Persona 2).
    pub oxygen01: f64,
    pub camera_position: Vec3,
    pub ship_position: Vec3,
    pub light_position: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Color lineal base.
    pub base_color: Vec3,
    /// Si es verdadero, cada cara usa el `Kd` de su material MTL en lugar de `base_color`.
    pub use_model_colors: bool,
    /// Fraccion de luz que recibe una cara en sombra (`0..=1`).
    pub ambient: f64,
    /// Descarta triangulos de espaldas (solo para mallas cerradas con orientacion antihoraria).
    pub cull_backfaces: bool,
}

impl Material {
    pub fn solid(r: f64, g: f64, b: f64) -> Self {
        Self {
            base_color: Vec3::new(r, g, b),
            use_model_colors: false,
            ambient: 0.04,
            cull_backfaces: false,
        }
    }
    pub fn from_model() -> Self {
        Self {
            base_color: Vec3::splat(0.7),
            use_model_colors: true,
            ambient: 0.3,
            cull_backfaces: false,
        }
    }
}

pub fn shade_fragment(
    kind: ShaderKind,
    fragment: &FragmentInput,
    context: &ShaderContext,
    material: &Material,
) -> Color {
    match kind {
        ShaderKind::Flat => material.base_color,
        // TODO(UMB-309): shader de Persona 3; mientras tanto usa el iluminado.
        ShaderKind::LitPlanet | ShaderKind::VoidEye => lit_planet(fragment, context, material),
        ShaderKind::LucidSurface => lucid_surface(fragment, context, material),
        ShaderKind::AnimatedSun => animated_sun(fragment, context, material),
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Difuso Lambert con terminador suave y un patron por UV (para que la rotacion se vea).
fn lit_planet(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let n = f.normal.normalized();
    let l = (c.light_position - f.world_position).normalized();
    let diffuse = smoothstep(-0.08, 0.35, n.dot(l));
    let pattern = 0.5
        + 0.25 * (f.uv.x * TAU * 6.0).sin() * (f.uv.y * PI * 4.0).sin()
        + 0.25 * (f.uv.y * PI * 9.0 + (f.uv.x * TAU * 3.0).sin()).sin();
    let albedo = m.base_color * (0.55 + 0.7 * pattern);
    albedo * (m.ambient + (1.0 - m.ambient) * diffuse)
}

/// Candidata UMB-308: el planeta iluminado con grietas rojo-ambar que crecen al bajar el
/// oxigeno. Entre grietas la piedra queda exactamente como `lit_planet`.
fn lucid_surface(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let base = lit_planet(f, c, m);
    // Un oxigeno no finito (NaN o +-Inf) se trata como estable: superficie sin grietas.
    if !c.oxygen01.is_finite() {
        return base;
    }
    let oxygen = c.oxygen01.clamp(0.0, 1.0);
    if oxygen >= LUCID_ONSET_OXYGEN {
        return base;
    }
    // Suave en ambos extremos: sin salto al cruzar el umbral y lleno desde el 5 %.
    let strength = 1.0 - smoothstep(LUCID_FULL_OXYGEN, LUCID_ONSET_OXYGEN, oxygen);
    // Direccion de la esfera unidad derivada de la UV (la convencion de `uv_sphere`): periodica
    // en u y sin singularidad en los polos, y anclada al planeta, no al espacio del mundo.
    let (sin_t, cos_t) = (f.uv.y * PI).sin_cos();
    let (sin_p, cos_p) = (f.uv.x * TAU).sin_cos();
    let direction = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
    let d = crack_edge_distance(direction);
    // Cada zona se enciende cuando la intensidad supera su umbral: las grietas aparecen por
    // zonas al bajar el oxigeno, no como un fundido uniforme. Las primeras zonas empiezan en
    // cuanto baja de 50 % y desde intensidad 0.8 todas estan encendidas.
    let threshold = 0.8 * smoothstep(0.2, 0.8, smooth_field(direction, 0.0));
    let on = strength * smoothstep(threshold - 0.2, threshold, strength);
    // Solo parte de la red llega a encenderse (venas, no una malla completa); el resto queda
    // como fisura oscura y fina. El brillo varia a lo largo de cada vena.
    let awake = smoothstep(0.35, 0.6, smooth_field(direction * 2.0, 5.0));
    let gain = awake * (0.55 + 0.45 * smooth_field(direction, 2.0));
    let width = CRACK_WIDTH * (0.5 + 0.5 * on);
    let line = 1.0 - smoothstep(0.0, width, d);
    // Borde carbonizado alrededor de la grieta; fuera de el `halo` es exactamente 0.
    let halo = 1.0 - smoothstep(0.0, 1.5 * width, d);
    let charred = 0.35 * on * halo * (0.4 + 0.6 * awake);
    let ember = EMBER_RED.lerp(EMBER_AMBER, line * line);
    // El tiempo solo modula el brillo de la grieta (respiracion lenta), nunca su posicion.
    // Tiempo no finito: como t = 0 (solo en este shader; los demas no cambian).
    let time = if c.elapsed_time.is_finite() {
        c.elapsed_time
    } else {
        0.0
    };
    let phase = (time / PULSE_PERIOD_SECONDS).rem_euclid(1.0);
    let pulse = 1.0 - PULSE_DEPTH * 0.5 * (1.0 - (phase * TAU).cos());
    base * (1.0 - charred) + ember * (line * on * gain * pulse)
}

const PULSE_PERIOD_SECONDS: f64 = 4.5;
/// Fraccion maxima en que baja el brillo durante la pulsacion (nunca se apaga).
const PULSE_DEPTH: f64 = 0.25;

/// Con este oxigeno o mas la superficie es exactamente `lit_planet`.
const LUCID_ONSET_OXYGEN: f64 = 0.5;
/// Con este oxigeno o menos las grietas estan en su maximo.
const LUCID_FULL_OXYGEN: f64 = 0.05;
/// Placas de piedra (celdas de Voronoi sobre la esfera) y su desorden.
const CRACK_SITES: usize = 48;
const CRACK_JITTER: f64 = 0.7;
/// Ondulacion del trazo y ancho maximo de la grieta, en radios del planeta.
const CRACK_WARP: f64 = 0.04;
const CRACK_WIDTH: f64 = 0.02;
const EMBER_RED: Vec3 = Vec3::new(0.55, 0.06, 0.02);
const EMBER_AMBER: Vec3 = Vec3::new(1.0, 0.42, 0.08);

/// Distancia al borde de la placa mas cercana (Voronoi sobre la esfera unidad), funcion
/// continua de la direccion; el trazo ondula un poco para no verse geometrico.
fn crack_edge_distance(direction: Vec3) -> f64 {
    let warp = Vec3::new(
        (direction.y * 7.3 + direction.z * 3.1).sin(),
        (direction.z * 6.7 - direction.x * 4.3).sin(),
        (direction.x * 7.9 + direction.y * 2.9).sin(),
    );
    let p = (direction + warp * CRACK_WARP).normalized();
    let sites = crack_sites();
    let nearest = (0..sites.len())
        .max_by(|&a, &b| p.dot(sites[a]).total_cmp(&p.dot(sites[b])))
        .unwrap_or(0);
    let near = sites[nearest];
    // Distancia a cada plano bisector (todos pasan por el centro): el borde de la celda.
    sites
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != nearest)
        .map(|(_, &s)| (p.dot(near) - p.dot(s)) / (near - s).length())
        .fold(f64::INFINITY, f64::min)
}

/// Campo suave en `0..=1` sobre la esfera (suma de senos de la direccion: sin costuras).
fn smooth_field(d: Vec3, seed: f64) -> f64 {
    let wave = 0.5 * (d.x * 2.3 + d.y * 1.1 + seed).sin()
        + 0.3 * (d.y * 3.7 - d.z * 2.9 + 1.7 * seed).sin()
        + 0.2 * (d.z * 5.3 + d.x * 4.1 - seed).sin();
    0.5 + 0.5 * wave
}

/// Centros de las placas: espiral de Fibonacci con desorden determinista, sobre la esfera.
fn crack_sites() -> &'static [Vec3] {
    static SITES: OnceLock<Vec<Vec3>> = OnceLock::new();
    SITES.get_or_init(|| {
        let n = CRACK_SITES as f64;
        let spacing = (4.0 * PI / n).sqrt();
        let golden = PI * (3.0 - 5f64.sqrt());
        (0..CRACK_SITES)
            .map(|i| {
                let y = 1.0 - 2.0 * (i as f64 + 0.5) / n;
                let r = (1.0 - y * y).sqrt();
                let (sin_a, cos_a) = (golden * i as f64).sin_cos();
                let jitter =
                    Vec3::new(unit_hash(i, 1), unit_hash(i, 2), unit_hash(i, 3)) - Vec3::splat(0.5);
                (Vec3::new(r * cos_a, y, r * sin_a) + jitter * (CRACK_JITTER * spacing))
                    .normalized()
            })
            .collect()
    })
}

/// Numero pseudoaleatorio determinista en `0..1` (splitmix64).
fn unit_hash(i: usize, salt: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt;
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Emisivo: turbulencia que cambia con `elapsed_time`, pulsacion y oscurecimiento del borde.
fn animated_sun(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let t = c.elapsed_time;
    let warp = (f.uv.y * PI * 6.0 + t * 1.3).sin() * 1.8;
    let pattern = 0.5 + 0.5 * (f.uv.x * TAU * 8.0 + t * 0.8 + warp).sin();
    let pattern = pattern * (0.6 + 0.4 * (f.uv.y * PI * 11.0 - t * 0.5).sin().abs());
    let view = (c.camera_position - f.world_position).normalized();
    let facing = f.normal.normalized().dot(view).abs().clamp(0.0, 1.0);
    let hot = Vec3::new(1.0, 0.95, 0.55);
    let pulse = 0.92 + 0.08 * (t * 2.0).sin();
    m.base_color.lerp(hot, pattern) * ((0.55 + 0.6 * facing.sqrt()) * pulse)
}

/// Lineal `0..=1` a sRGB de 8 bits empaquetado `0xRRGGBB`, con tabla para no llamar `powf`.
pub fn to_rgb8(color: Color) -> u32 {
    static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
    const STEPS: usize = 4096;
    let table = TABLE.get_or_init(|| {
        (0..=STEPS)
            .map(|i| {
                let l = i as f64 / STEPS as f64;
                let s = if l <= 0.003_130_8 {
                    l * 12.92
                } else {
                    1.055 * l.powf(1.0 / 2.4) - 0.055
                };
                (s * 255.0 + 0.5) as u8
            })
            .collect()
    });
    let ch = |v: f64| {
        let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        table[(v * STEPS as f64 + 0.5) as usize] as u32
    };
    (ch(color.x) << 16) | (ch(color.y) << 8) | ch(color.z)
}
