//! Sol y planetas con orbita y rotacion dependientes del tiempo (UMB-107).
//! Propietario: Persona 1 (movimiento); Persona 3 agrega contenido por planeta.

use crate::{
    math::{Transform, Vec3},
    renderer::shader::{Material, ShaderKind},
};
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct OrbitSettings {
    /// Distancia al Sol (0 para el propio Sol).
    pub radius: f64,
    /// Rad/s alrededor del Sol.
    pub angular_speed: f64,
    /// Rad/s sobre el eje propio.
    pub axial_speed: f64,
}

#[derive(Clone, Debug)]
pub struct CelestialBody {
    pub name: &'static str,
    pub body_radius: f64,
    pub orbit: OrbitSettings,
    /// Angulo orbital en t = 0.
    pub phase: f64,
    pub material: Material,
    pub shader: ShaderKind,
}

impl CelestialBody {
    /// Posicion en mundo en el instante `t` (segundos); el Sol esta en el origen.
    pub fn position(&self, t: f64) -> Vec3 {
        let a = self.phase + self.orbit.angular_speed * t;
        Vec3::new(a.cos(), 0.0, a.sin()) * self.orbit.radius
    }
    pub fn rotation_angle(&self, t: f64) -> f64 {
        (self.orbit.axial_speed * t).rem_euclid(TAU)
    }
    /// Transformacion de dibujo: esfera unitaria escalada al radio del cuerpo.
    pub fn transform(&self, t: f64) -> Transform {
        Transform {
            position: self.position(t),
            rotation: Vec3::new(0.0, self.rotation_angle(t), 0.0),
            scale: Vec3::splat(self.body_radius),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SolarSystem {
    pub bodies: Vec<CelestialBody>,
}

impl SolarSystem {
    pub fn light_position(&self) -> Vec3 {
        Vec3::ZERO
    }
    /// Sistema de demostracion: Sol en el centro y tres mundos con radios y orbitas
    /// reducidos para acortar el viaje. TODO(UMB-302): identidad propia por planeta.
    pub fn demo() -> Self {
        let body = |name: &'static str,
                    body_radius: f64,
                    radius: f64,
                    angular_speed: f64,
                    axial_speed: f64,
                    phase: f64,
                    color: (f64, f64, f64),
                    shader: ShaderKind| {
            CelestialBody {
                name,
                body_radius,
                orbit: OrbitSettings {
                    radius,
                    angular_speed,
                    axial_speed,
                },
                phase,
                material: Material::solid(color.0, color.1, color.2),
                shader,
            }
        };
        Self {
            bodies: vec![
                body(
                    "Sol",
                    60.0,
                    0.0,
                    0.0,
                    0.05,
                    0.0,
                    (1.0, 0.45, 0.08),
                    ShaderKind::AnimatedSun,
                ),
                body(
                    "Antena",
                    14.0,
                    220.0,
                    0.060,
                    0.40,
                    0.6,
                    (0.45, 0.40, 0.36),
                    ShaderKind::LitPlanet,
                ),
                body(
                    "Osario",
                    9.0,
                    340.0,
                    0.040,
                    0.80,
                    2.6,
                    (0.62, 0.55, 0.42),
                    ShaderKind::LitPlanet,
                ),
                body(
                    "Vigilia",
                    20.0,
                    520.0,
                    0.025,
                    0.25,
                    4.4,
                    (0.30, 0.38, 0.55),
                    ShaderKind::LitPlanet,
                ),
            ],
        }
    }
}
