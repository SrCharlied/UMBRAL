//! Nave con movimiento minimo obligatorio (UMB-106). Persona 1; Persona 2 lo refina en
//! UMB-201 sin romper esta API. Todo depende de `dt`, nunca del numero de cuadros.

use crate::math::{Mat4, Transform, Vec3};

/// Entradas normalizadas `-1..=1`; el origen (teclado, mando) no importa a la nave.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShipInput {
    /// +1 acelera hacia adelante, -1 frena/retrocede.
    pub thrust: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub boost: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ShipSettings {
    pub max_speed: f64,
    pub boost_multiplier: f64,
    pub reverse_fraction: f64,
    /// Constante de suavizado de la velocidad (1/s); mayor = respuesta mas rapida.
    pub speed_response: f64,
    /// Rad/s a entrada completa.
    pub turn_rate: f64,
    pub turn_response: f64,
}

impl Default for ShipSettings {
    fn default() -> Self {
        Self {
            max_speed: 60.0,
            boost_multiplier: 5.0,
            reverse_fraction: 0.25,
            speed_response: 1.5,
            turn_rate: 1.4,
            turn_response: 6.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ship {
    pub position: Vec3,
    /// Rotacion de nave a mundo; la nave apunta a su -Z local.
    pub orientation: Mat4,
    pub speed: f64,
    /// Velocidad angular suavizada (cabeceo, guinada, alabeo) en rad/s.
    angular: Vec3,
    pub settings: ShipSettings,
}

/// Factor de suavizado exponencial independiente de la tasa de cuadros.
pub fn smoothing(rate: f64, dt: f64) -> f64 {
    1.0 - (-rate * dt).exp()
}

impl Ship {
    pub fn new(position: Vec3, forward: Vec3) -> Self {
        // Misma base que una camara mirando a `forward`, invertida a mundo.
        let view = Mat4::look_at(Vec3::ZERO, forward, Vec3::Y);
        let mut orientation = Mat4::IDENTITY;
        for r in 0..3 {
            for c in 0..3 {
                orientation.m[r][c] = view.m[c][r];
            }
        }
        Self {
            position,
            orientation,
            speed: 0.0,
            angular: Vec3::ZERO,
            settings: ShipSettings::default(),
        }
    }
    pub fn forward(&self) -> Vec3 {
        self.orientation.transform_dir(-Vec3::Z)
    }
    pub fn up(&self) -> Vec3 {
        self.orientation.transform_dir(Vec3::Y)
    }
    pub fn right(&self) -> Vec3 {
        self.orientation.transform_dir(Vec3::X)
    }
    pub fn update(&mut self, input: &ShipInput, dt: f64) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let s = self.settings;
        let top = s.max_speed * if input.boost { s.boost_multiplier } else { 1.0 };
        let target_speed = match input.thrust.clamp(-1.0, 1.0) {
            t if t >= 0.0 => t * top,
            t => t * s.max_speed * s.reverse_fraction,
        };
        self.speed += (target_speed - self.speed) * smoothing(s.speed_response, dt);

        let target = Vec3::new(input.pitch, input.yaw, input.roll) * s.turn_rate;
        self.angular += (target - self.angular) * smoothing(s.turn_response, dt);
        // Giro en ejes locales: sin bloqueo de cardan porque se acumula una matriz.
        let step = Mat4::rotation_axis(Vec3::X, self.angular.x * dt)
            * Mat4::rotation_axis(Vec3::Y, self.angular.y * dt)
            * Mat4::rotation_axis(Vec3::Z, self.angular.z * dt);
        self.orientation = (self.orientation * step).orthonormalized();
        self.position += self.forward() * (self.speed * dt);
    }
    /// Transformacion de dibujo; `model_yaw` corrige hacia donde mira el modelo (D11).
    pub fn transform(&self, model_yaw: f64) -> Transform {
        let rotation = (self.orientation * Mat4::rotation_y(model_yaw)).to_euler();
        Transform {
            position: self.position,
            rotation,
            scale: Vec3::ONE,
        }
    }
}
