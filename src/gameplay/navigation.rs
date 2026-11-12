//! Navegacion asistida, gravedad opcional y colisiones (UMB-202..204).
use crate::{
    math::{Mat4, Vec3},
    scene::{
        ship::{Ship, ShipInput, nonnegative, smoothing, valid_dt},
        solar_system::SolarSystem,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct NavigationSettings {
    pub ship_radius: f64,
    pub orbit_gap: f64,
    pub orbit_capture_gap: f64,
    pub orbit_angular_speed: f64,
    pub orbit_response: f64,
    /// Multiplicador por planeta (masa aproximada = radio al cuadrado); cero desactiva.
    pub gravity_strength: f64,
    pub max_gravity_acceleration: f64,
}
impl Default for NavigationSettings {
    fn default() -> Self {
        Self {
            ship_radius: 2.0,
            orbit_gap: 20.0,
            orbit_capture_gap: 100.0,
            orbit_angular_speed: 0.45,
            orbit_response: 2.0,
            gravity_strength: 0.0,
            max_gravity_acceleration: 30.0,
        }
    }
}
impl NavigationSettings {
    fn safe(self) -> Self {
        let d = Self::default();
        Self {
            ship_radius: nonnegative(self.ship_radius, d.ship_radius),
            orbit_gap: nonnegative(self.orbit_gap, d.orbit_gap),
            orbit_capture_gap: nonnegative(self.orbit_capture_gap, d.orbit_capture_gap),
            orbit_angular_speed: if self.orbit_angular_speed.is_finite() {
                self.orbit_angular_speed
            } else {
                d.orbit_angular_speed
            },
            orbit_response: nonnegative(self.orbit_response, d.orbit_response),
            gravity_strength: nonnegative(self.gravity_strength, 0.0),
            max_gravity_acceleration: nonnegative(
                self.max_gravity_acceleration,
                d.max_gravity_acceleration,
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NavigationState {
    pub settings: NavigationSettings,
    pub assisted_planet: Option<usize>,
    pub gravity_velocity: Vec3,
}
impl NavigationState {
    /// Los pasos invalidos o mayores a 60s se ignoran, igual que Ship::update.
    pub fn apply_gravity(&mut self, ship: &mut Ship, system: &SolarSystem, time: f64, dt: f64) {
        if !valid_dt(dt) || !time.is_finite() || !ship.position.is_finite() {
            return;
        }
        let s = self.settings.safe();
        // La asistencia controla la trayectoria: no debe almacenar impulso invisible.
        if s.gravity_strength == 0.0 || self.assisted_planet.is_some() {
            self.gravity_velocity = Vec3::ZERO;
            return;
        }
        if !self.gravity_velocity.is_finite() {
            self.gravity_velocity = Vec3::ZERO;
        }
        let mut acceleration = Vec3::ZERO;
        for body in &system.bodies {
            if !body.body_radius.is_finite() || body.body_radius <= 0.0 {
                continue;
            }
            let offset = body.position(time) - ship.position;
            let distance = offset.length().max(body.body_radius + s.ship_radius);
            if !offset.is_finite() || !distance.is_finite() {
                continue;
            }
            // Cociente antes del cuadrado: no desborda strength * radius^2.
            let ratio = body.body_radius / distance;
            let magnitude = (s.gravity_strength * ratio * ratio).min(s.max_gravity_acceleration);
            acceleration += offset.normalized() * magnitude;
        }
        let length = acceleration.length();
        if !length.is_finite() {
            return;
        }
        if length > s.max_gravity_acceleration {
            acceleration = acceleration.normalized() * s.max_gravity_acceleration;
        }
        let position = ship.position + self.gravity_velocity * dt + acceleration * (0.5 * dt * dt);
        let velocity = self.gravity_velocity + acceleration * dt;
        if position.is_finite() && velocity.is_finite() {
            ship.position = position;
            self.gravity_velocity = velocity;
        }
    }

    /// Asistencia geometrica; entrada manual o invalida devuelve el control al piloto.
    /// Conserva el plano en ship.up() y reinicia giro/impulso al tomar el control.
    #[allow(clippy::too_many_arguments)]
    pub fn assist(
        &mut self,
        ship: &mut Ship,
        input: &ShipInput,
        system: &SolarSystem,
        old_time: f64,
        time: f64,
        dt: f64,
        enabled: bool,
    ) {
        if !valid_dt(dt) || !old_time.is_finite() || !time.is_finite() {
            return;
        }
        let previous_planet = self.assisted_planet.take();
        if !enabled
            || !ship.position.is_finite()
            || input.boost
            || [input.thrust, input.yaw, input.pitch, input.roll]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 0.01)
        {
            return;
        }
        let s = self.settings.safe();
        let candidate = |index: usize| {
            let b = system.bodies.get(index)?;
            if index == 0 || !b.body_radius.is_finite() || b.body_radius <= 0.0 {
                return None;
            }
            let gap = (ship.position - b.position(old_time)).length() - b.body_radius;
            let capture_gap = if previous_planet == Some(index) {
                s.orbit_capture_gap.max(s.orbit_gap)
            } else {
                s.orbit_capture_gap
            };
            (gap.is_finite() && gap <= capture_gap).then_some((index, gap))
        };
        // Mantener el planeta capturado evita cambios de plano al cruzar otra zona.
        let nearest = previous_planet.and_then(candidate).or_else(|| {
            (1..system.bodies.len())
                .filter_map(candidate)
                .min_by(|a, b| a.1.total_cmp(&b.1))
        });
        let Some((index, _)) = nearest else {
            return;
        };
        let body = &system.bodies[index];
        let offset = ship.position - body.position(old_time);
        let radial = offset.normalized();
        let radial = if radial == Vec3::ZERO {
            Vec3::X
        } else {
            radial
        };
        let preferred_axis = if previous_planet == Some(index) {
            ship.up()
        } else if radial.dot(Vec3::Y).abs() > 0.95 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        // Un eje perpendicular produce un circulo completo tambien desde los polos.
        let axis = (preferred_axis - radial * preferred_axis.dot(radial)).normalized();
        if axis == Vec3::ZERO {
            return;
        }
        let angle = s.orbit_angular_speed * dt;
        if !angle.is_finite() {
            return;
        }
        let next = Mat4::rotation_axis(axis, angle)
            .transform_dir(radial)
            .normalized();
        let safe_radius = body.body_radius + s.ship_radius + 1.0;
        let target = (body.body_radius + s.orbit_gap).max(safe_radius);
        let radius = (offset.length()
            + (target - offset.length()) * smoothing(s.orbit_response, dt))
        .max(safe_radius);
        let position = body.position(time) + next * radius;
        if !position.is_finite() {
            return;
        }
        let tangent = axis.cross(next).normalized()
            * if s.orbit_angular_speed < 0.0 {
                -1.0
            } else {
                1.0
            };
        // Base con up orbital fijo: evita el salto de look_at al cruzar el polo.
        let right = tangent.cross(axis).normalized();
        let up = right.cross(tangent);
        let mut orientation = Mat4::IDENTITY;
        for (c, v) in [right, up, -tangent].into_iter().enumerate() {
            orientation.m[0][c] = v.x;
            orientation.m[1][c] = v.y;
            orientation.m[2][c] = v.z;
        }
        let settings = ship.settings;
        *ship = Ship::new(position, tangent); // limpia tambien la velocidad angular privada
        ship.settings = settings;
        ship.orientation = orientation;
        self.gravity_velocity = Vec3::ZERO;
        self.assisted_planet = Some(index);
    }

    /// Barrido relativo lineal entre centros en old_time/time; incluye contacto inicial.
    /// Se resuelve primero el impacto mas temprano, no el orden del catalogo.
    pub fn resolve_collisions(
        &mut self,
        ship: &mut Ship,
        previous: Vec3,
        system: &SolarSystem,
        old_time: f64,
        time: f64,
    ) {
        if !previous.is_finite()
            || !ship.position.is_finite()
            || !old_time.is_finite()
            || !time.is_finite()
        {
            return;
        }
        let s = self.settings.safe();
        let mut first: Option<(f64, Vec3, Vec3, f64)> = None;
        for body in &system.bodies {
            if !body.body_radius.is_finite() || body.body_radius <= 0.0 {
                continue;
            }
            let center = body.position(time);
            let start = previous - body.position(old_time);
            let end = ship.position - center;
            let radius = body.body_radius + s.ship_radius;
            if !center.is_finite() || !start.is_finite() || !radius.is_finite() {
                continue;
            }
            let delta = end - start;
            let a = delta.dot(delta);
            let b = start.dot(delta);
            let c = start.dot(start) - radius * radius;
            let discriminant = b * b - a * c;
            let t = if c < 0.0 && b < 0.0 {
                // Un inicio ya penetrado tampoco puede cruzar hacia el lado opuesto;
                // conservar la normal de entrada permite salir por el lado original.
                Some(0.0)
            } else if c >= 0.0 && a > 1e-16 && b < 0.0 && discriminant >= 0.0 {
                // Forma estable de la raiz de entrada (evita restar numeros cercanos).
                let denominator = -b + discriminant.sqrt();
                let t = c / denominator;
                (0.0..=1.0).contains(&t).then_some(t)
            } else {
                None
            };
            if let Some(t) = t
                && first.as_ref().is_none_or(|hit| t < hit.0)
            {
                first = Some((t, center, start + delta * t, radius));
            }
        }
        if let Some((_, center, normal, radius)) = first {
            self.separate(ship, center, normal, radius);
        }
        // Contacto discreto/inicio dentro: corregir penetracion aunque no exista raiz.
        // Varias pasadas cubren cuerpos proximos sin repetir el barrido original.
        for _ in 0..system.bodies.len().min(8) {
            let mut corrected = false;
            for body in &system.bodies {
                if !body.body_radius.is_finite() || body.body_radius <= 0.0 {
                    continue;
                }
                let center = body.position(time);
                let end = ship.position - center;
                let radius = body.body_radius + s.ship_radius;
                if radius.is_finite() && center.is_finite() && end.length() < radius {
                    self.separate(ship, center, end, radius);
                    corrected = true;
                }
            }
            if !corrected {
                break;
            }
        }
    }

    fn separate(&mut self, ship: &mut Ship, center: Vec3, normal: Vec3, radius: f64) {
        let normal = normal.normalized();
        let normal = if normal == Vec3::ZERO {
            Vec3::Y
        } else {
            normal
        };
        let position = center + normal * (radius + 1e-7);
        if !position.is_finite() {
            return;
        }
        ship.position = position;
        if (ship.forward() * ship.speed).dot(normal) < 0.0 {
            ship.speed = 0.0;
        }
        if !self.gravity_velocity.is_finite() {
            self.gravity_velocity = Vec3::ZERO;
        }
        let inward = self.gravity_velocity.dot(normal).min(0.0);
        self.gravity_velocity = self.gravity_velocity - normal * inward;
    }
}
