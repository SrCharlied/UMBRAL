//! Muerte por el monstruo (experiencia de presentacion con «La Vigilia»): cuando el O2 REAL
//! llega a 0, en lugar de morir en el acto empieza la secuencia de muerte:
//!
//! 1. **Te jala:** el monstruo toma el control de la nave y la lleva hacia el (centro del Sol)
//!    hasta dejarla al alcance del tentaculo (`pull_input`).
//! 2. **Te atrapa y arrastra:** la captura automatica APROBADA (`tentacle_auto_capture`) hace su
//!    contacto, agarre y arrastre sin cambios.
//! 3. **Te mata:** con la nave retenida (`AutoPhase::Held`), tras `HOLD_BEFORE_DEATH` segundos la
//!    partida termina en muerte y aparece la presentacion final.
//!
//! Durante la secuencia el O2 real se sostiene en `DEATH_OXYGEN` (para que el consumo no la
//! interrumpa) y el mostrado tambien: la percepcion no deja la pantalla en negro y el tentaculo
//! persigue a su maxima rapidez (umbral de persecucion 15 %). El HUD muestra 0 % (`World::dying`).
//! Si la captura no llega en `FALLBACK_DEATH_SECONDS`, la muerte llega igual.

use crate::{
    math::Vec3,
    scene::{ship::ShipInput, world::World},
    visual::tentacle_auto_capture::AutoPhase,
};

/// O2 (real y mostrado) sostenido durante la secuencia; dispara la secuencia al alcanzarlo.
pub const DEATH_OXYGEN: f64 = 0.03;
/// Segundos con la nave retenida antes de la muerte.
pub const HOLD_BEFORE_DEATH: f64 = 1.5;
/// Muerte de respaldo si la captura no ocurre.
pub const FALLBACK_DEATH_SECONDS: f64 = 25.0;
/// Distancia al centro del monstruo donde el piloto automatico deja de empujar.
pub const PULL_STOP_DISTANCE: f64 = 300.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MonsterDeath {
    /// Segundos reales desde que empezo la secuencia (`None`: no empezo).
    started: Option<f64>,
    /// Segundos reales con la nave retenida.
    held: f64,
}

impl MonsterDeath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> bool {
        self.started.is_some()
    }

    /// Antes del cuadro: arranca o sostiene la secuencia y devuelve la entrada de vuelo
    /// efectiva (la del jugador, o la del monstruo que jala si la secuencia esta en curso y la
    /// nave sigue libre).
    pub fn before_frame(
        &mut self,
        world: &mut World,
        player: ShipInput,
        phase: AutoPhase,
    ) -> ShipInput {
        if world.gameplay.game_over || world.gameplay.objective_complete {
            return player;
        }
        if !self.active() && world.gameplay.oxygen01 <= DEATH_OXYGEN {
            self.started = Some(0.0);
        }
        if !self.active() {
            return player;
        }
        self.hold_oxygen(world);
        if phase == AutoPhase::Pursuit
            || phase == AutoPhase::Observation
            || phase == AutoPhase::Aborted
        {
            pull_input(world, monster_center(world))
        } else {
            ShipInput::default()
        }
    }

    /// Despues del cuadro: sostiene el O2, cuenta el tiempo y decide la muerte. Devuelve `true`
    /// en el cuadro en que el monstruo mata.
    pub fn after_frame(&mut self, world: &mut World, phase: AutoPhase, elapsed: f64) -> bool {
        if world.gameplay.game_over || world.gameplay.objective_complete {
            return false;
        }
        // El cuadro pudo bajar el O2 a 0 (N o un paso largo): la secuencia empieza aqui.
        if !self.active() && world.gameplay.oxygen01 <= DEATH_OXYGEN {
            self.started = Some(0.0);
        }
        let Some(started) = self.started.as_mut() else {
            world.dying = false;
            return false;
        };
        let dt = if elapsed.is_finite() {
            elapsed.clamp(0.0, 0.25)
        } else {
            0.0
        };
        *started += dt;
        let since = *started;
        self.hold_oxygen(world);
        if phase == AutoPhase::Held {
            self.held += dt;
        }
        if self.held >= HOLD_BEFORE_DEATH || since >= FALLBACK_DEATH_SECONDS {
            world.gameplay.oxygen01 = 0.0;
            world.gameplay.game_over = true;
            world.dying = false;
            return true;
        }
        false
    }

    /// Partida nueva.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn hold_oxygen(&self, world: &mut World) {
        world.dying = true;
        world.gameplay.oxygen01 = DEATH_OXYGEN;
        world.forced_oxygen = Some(DEATH_OXYGEN);
    }
}

/// Centro del monstruo: el del Sol (la criatura vive en su centro).
pub fn monster_center(world: &World) -> Vec3 {
    world
        .system
        .bodies
        .first()
        .map_or(Vec3::ZERO, |sun| sun.position(world.time))
}

/// Piloto automatico del monstruo: gira la nave hacia `target` y la empuja hasta
/// `PULL_STOP_DISTANCE`; ahi frena para que el tentaculo la alcance.
pub fn pull_input(world: &World, target: Vec3) -> ShipInput {
    let ship = &world.ship;
    let to = target - ship.position;
    let distance = to.length();
    let dir = if distance > 1e-9 {
        to / distance
    } else {
        ship.forward()
    };
    let (x, y, z) = (
        dir.dot(ship.right()),
        dir.dot(ship.up()),
        dir.dot(ship.forward()),
    );
    // A gira a la izquierda (guinada positiva): para ir a la derecha la guinada es negativa.
    let mut yaw = (-x * 3.0).clamp(-1.0, 1.0);
    if z < 0.0 && x.abs() < 0.2 {
        yaw = -1.0; // detras: girar decididamente
    }
    let pitch = (y * 3.0).clamp(-1.0, 1.0);
    let (thrust, boost) = if distance > PULL_STOP_DISTANCE {
        (
            if z > 0.6 { 1.0 } else { 0.15 },
            distance > 900.0 && z > 0.95,
        )
    } else {
        (if ship.speed > 1.0 { -1.0 } else { 0.0 }, false)
    };
    ShipInput {
        thrust,
        yaw,
        pitch,
        roll: 0.0,
        boost,
    }
}
