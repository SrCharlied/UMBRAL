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
        // TODO(UMB-308/309): shaders de Persona 3; mientras tanto usan el iluminado.
        ShaderKind::LitPlanet | ShaderKind::LucidSurface | ShaderKind::VoidEye => {
            lit_planet(fragment, context, material)
        }
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
