//! Gameplay independiente del render. Unidades: segundos y unidades de mundo.
pub mod collectibles;
pub mod navigation;
use crate::{math::Vec3, scene::solar_system::SolarSystem};
pub use collectibles::{Collectible, NarrativeLog};
pub use navigation::{NavigationSettings, NavigationState};
#[derive(Clone, Debug)]
pub struct GameplayState {
    pub oxygen01: f64,
    pub collected_resources: u32,
    pub collected_logs: u32,
    pub game_over: bool,
    pub objective_complete: bool,
    pub settings: GameplaySettings,
    pub navigation: NavigationState,
    pub resources: Vec<Collectible>,
    pub logs: Vec<NarrativeLog>,
    /// IDs recogidos en el ultimo update: feedback para HUD/audio.
    pub resource_events: Vec<usize>,
    pub log_events: Vec<usize>,
    /// Ultimo registro recogido; persiste entre cuadros hasta reiniciar.
    pub last_log: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplaySettings {
    pub drain_per_second: f64,
    pub pickup_recovery: f64,
    pub pickup_radius: f64,
    pub sun_arrival_gap: f64,
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            drain_per_second: 1.0 / 420.0,
            pickup_recovery: 0.15,
            pickup_radius: 8.0,
            sun_arrival_gap: 30.0,
        }
    }
}

impl GameplaySettings {
    /// Configuracion editable: valores no finitos vuelven al default; negativos a cero.
    pub fn sanitized(self) -> Self {
        let defaults = Self::default();
        let valid = |value: f64, fallback: f64| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                fallback
            }
        };
        Self {
            drain_per_second: valid(self.drain_per_second, defaults.drain_per_second),
            pickup_recovery: valid(self.pickup_recovery, defaults.pickup_recovery).min(1.0),
            pickup_radius: valid(self.pickup_radius, defaults.pickup_radius),
            sun_arrival_gap: valid(self.sun_arrival_gap, defaults.sun_arrival_gap),
        }
    }
}

impl Default for GameplayState {
    fn default() -> Self {
        let (resources, logs) = collectibles::catalog();
        Self {
            oxygen01: 1.0,
            collected_resources: 0,
            collected_logs: 0,
            game_over: false,
            objective_complete: false,
            settings: GameplaySettings::default(),
            navigation: NavigationState::default(),
            resources,
            logs,
            resource_events: Vec::new(),
            log_events: Vec::new(),
            last_log: None,
        }
    }
}

impl GameplayState {
    /// Recoge por barrido entre posiciones de mundo e instantes orbitales (segundos).
    /// Aproxima el movimiento del ancla por su segmento entre ambos instantes.
    /// Con orbitas congeladas, pasar el mismo tiempo en ambos extremos.
    /// La victoria exige terminar cerca del Sol, incluso si se lo atraveso en el paso.
    pub fn collect_along(
        &mut self,
        start: Vec3,
        end: Vec3,
        system: &SolarSystem,
        start_time: f64,
        end_time: f64,
    ) {
        self.resource_events.clear();
        self.log_events.clear();
        if !self.prepare_active()
            || !start.is_finite()
            || !end.is_finite()
            || !start_time.is_finite()
            || !end_time.is_finite()
            || end_time < start_time
        {
            return;
        }
        let touched = |anchor: &Collectible| {
            let Some(a) = anchor.position(system, start_time) else {
                return false;
            };
            let Some(b) = anchor.position(system, end_time) else {
                return false;
            };
            segment_near_origin(start - a, end - b, self.settings.pickup_radius)
        };
        for resource in &mut self.resources {
            if !resource.collected && touched(resource) {
                resource.collected = true;
                self.collected_resources += 1;
                self.oxygen01 =
                    (self.oxygen01 + self.settings.pickup_recovery.max(0.0)).clamp(0.0, 1.0);
                self.resource_events.push(resource.id);
            }
        }
        for log in &mut self.logs {
            if !log.anchor.collected && touched(&log.anchor) {
                log.anchor.collected = true;
                self.collected_logs += 1;
                self.log_events.push(log.anchor.id);
                self.last_log = Some(log.anchor.id);
            }
        }
        if self.collected_logs == 3
            && system.bodies.first().is_some_and(|sun| {
                sun.body_radius.is_finite()
                    && sun.body_radius >= 0.0
                    && (end - sun.position(end_time)).length()
                        <= sun.body_radius + self.settings.sun_arrival_gap
            })
        {
            self.objective_complete = true;
        }
    }

    /// Caso puntual del mismo algoritmo: conserva los eventos y el ultimo registro.
    pub fn collect_near(&mut self, position: Vec3, system: &SolarSystem, time: f64) {
        self.collect_along(position, position, system, time, time);
    }

    pub fn tick(&mut self, dt: f64) {
        // Reparar campos publicos aun si el reloj recibido es invalido.
        if !self.prepare_active() || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.oxygen01 =
            (self.oxygen01 - self.settings.drain_per_second.max(0.0) * dt).clamp(0.0, 1.0);
        if self.oxygen01 < 1e-12 {
            self.oxygen01 = 0.0;
            self.game_over = true;
        }
    }

    fn prepare_active(&mut self) -> bool {
        self.settings = self.settings.sanitized();
        self.oxygen01 = if self.oxygen01.is_finite() {
            self.oxygen01.clamp(0.0, 1.0)
        } else {
            0.0
        };
        // La muerte es terminal y prevalece si ambos flags publicos fueron activados.
        if self.game_over {
            self.oxygen01 = 0.0;
            self.objective_complete = false;
            return false;
        }
        if self.objective_complete {
            return false;
        }
        if self.oxygen01 == 0.0 {
            self.game_over = true;
        }
        !self.game_over
    }
}

fn segment_near_origin(start: Vec3, end: Vec3, radius: f64) -> bool {
    let delta = end - start;
    let length_squared = delta.dot(delta);
    if !start.is_finite() || !end.is_finite() || !length_squared.is_finite() {
        return false;
    }
    let t = if length_squared > 0.0 {
        (-start.dot(delta) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (start + delta * t).length() <= radius
}
