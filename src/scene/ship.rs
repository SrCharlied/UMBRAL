//! Control progresivo (UMB-201), compatible con la API de UMB-106.

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
    if !rate.is_finite() || rate <= 0.0 || !dt.is_finite() || dt <= 0.0 {
        return 0.0;
    }
    -(-rate * dt).exp_m1()
}

/// Un paso de vuelo admite hasta un minuto; pausas mayores deben dividirse en el caller.
/// Evita desbordamientos y trabajo ilimitado ante tiempos corruptos.
pub(crate) fn valid_dt(dt: f64) -> bool {
    dt.is_finite() && dt > 0.0 && dt <= 60.0
}

pub(crate) fn nonnegative(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback
    }
}

impl Ship {
    pub fn new(position: Vec3, forward: Vec3) -> Self {
        let forward = forward.normalized();
        let forward = if forward == Vec3::ZERO {
            -Vec3::Z
        } else {
            forward
        };
        // Misma base que una camara mirando a `forward`, invertida a mundo.
        let view = Mat4::look_at(Vec3::ZERO, forward, Vec3::Y);
        let mut orientation = Mat4::IDENTITY;
        for r in 0..3 {
            for c in 0..3 {
                orientation.m[r][c] = view.m[c][r];
            }
        }
        Self {
            position: if position.is_finite() {
                position
            } else {
                Vec3::ZERO
            },
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
        if !valid_dt(dt) {
            return;
        }
        let s = self.settings;
        let defaults = ShipSettings::default();
        let max_speed = nonnegative(s.max_speed, defaults.max_speed);
        let boost = nonnegative(s.boost_multiplier, defaults.boost_multiplier).max(1.0);
        let top = max_speed * if input.boost { boost } else { 1.0 };
        let normalized = |v: f64| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        let target_speed = match normalized(input.thrust) {
            t if t >= 0.0 => t * top,
            t => {
                t * max_speed * nonnegative(s.reverse_fraction, defaults.reverse_fraction).min(1.0)
            }
        };
        let rate = nonnegative(s.speed_response, defaults.speed_response);
        let turn_response = nonnegative(s.turn_response, defaults.turn_response);
        let target = Vec3::new(
            normalized(input.pitch),
            normalized(input.yaw),
            normalized(input.roll),
        ) * nonnegative(s.turn_rate, defaults.turn_rate);
        // Subpasos acotados para integrar trayectoria curva; las velocidades se integran
        // exactamente y el desplazamiento usa la orientacion en el punto medio.
        let count = (dt * 240.0).ceil() as usize;
        let h = dt / count as f64;
        let mut next = *self;
        for _ in 0..count {
            let factor = smoothing(rate, h);
            let distance =
                next.speed * h + (target_speed - next.speed) * integrated_response(rate, h);
            next.speed += (target_speed - next.speed) * factor;
            let angle =
                next.angular * h + (target - next.angular) * integrated_response(turn_response, h);
            let half_angle = next.angular * (h * 0.5)
                + (target - next.angular) * integrated_response(turn_response, h * 0.5);
            let rotate = |v: Vec3| {
                let length = v.length();
                if length == 0.0 {
                    Mat4::IDENTITY
                } else {
                    Mat4::rotation_axis(v, length)
                }
            };
            let mid = next.orientation * rotate(half_angle);
            next.position += mid.transform_dir(-Vec3::Z) * distance;
            next.orientation = (next.orientation * rotate(angle)).orthonormalized();
            next.angular += (target - next.angular) * smoothing(turn_response, h);
            if !next.position.is_finite()
                || !next.speed.is_finite()
                || !next.angular.is_finite()
                || !next.orientation.m.iter().flatten().all(|x| x.is_finite())
            {
                return; // El paso se confirma entero o se conserva el estado anterior.
            }
        }
        *self = next;
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

/// Integral de 1-exp(-rate*t), estable tambien cuando rate*dt es muy pequeno.
fn integrated_response(rate: f64, dt: f64) -> f64 {
    let x = rate * dt;
    if x < 1e-4 {
        dt * (x * 0.5 - x * x / 6.0 + x * x * x / 24.0)
    } else {
        dt - smoothing(rate, dt) / rate
    }
}
