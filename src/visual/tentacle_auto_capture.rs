//! GT3-AUTO: captura AUTOMATICA por contacto en el juego principal (`--tentacle-auto-capture`, opt-in). Une la
//! persecucion espacial existente con el gesto aprobado (B1/P1) y el arrastre aprobado (GT1-CARRY) SIN tecla
//! `T` ni quietud del jugador. Ver `docs/tentacle-auto-capture.md`.
//!
//! - Antes del contacto: vuelo y controlador reales libres; la persecucion es la de siempre.
//! - Contacto: el paso de persecucion publicado en ESTE cuadro sobre el World ya movido llega a la nave
//!   (`fresh_contact`) y B1 arranca con las matrices reales (`GraspPreview::frame_on_contact`). En el MISMO
//!   cuadro la nave queda capturada donde ESTA, sin velocidad ni inercia (`World::commit_captured_ship_pose`).
//! - Cierre B1/P1 (gesto aprobado, ~8 s): el nucleo exige las matrices capturadas BIT A BIT. Referencias
//!   cinematicas inmovilizadas y DECLARADAS: la nave se re-publica en su pose de contacto cada cuadro y el
//!   seguimiento corporal no avanza (`World::update_creature_eye_follow_body_held`: estado exacto conservado,
//!   sin reinicio). Mundo, O2, planetas, ojo, camara de persecucion, post y tiempo siguen.
//! - Gripped: bloqueo, arrastre 3,5 s y retencion APROBADOS (mismas funciones); el seguimiento se reanuda.
//! - Vuelo y R/7/8 bloqueados desde el contacto. O2 > 15 % suelta (observacion) sin inercia; un error suelta
//!   o aborta: nunca queda la nave bloqueada sin cierre vivo. X = escape de DIAGNOSTICO de siempre.
//! - Re-armado: una observacion (o aborto) vuelve a la persecucion en reposo el primer cuadro en que la capa
//!   no se dibuja (ojo oculto, sin salto visible), con el mismo reinicio que X.
// Bucles por indice sobre componentes xyz: estilo, no logica.
#![allow(clippy::needless_range_loop)]

use crate::{
    app::{MAX_PHYSICS_DT, advance_perception_frame_owned},
    math::{Mat4, Vec3},
    scene::{
        ship::{Ship, ShipInput},
        world::{SHIP_MODEL_YAW, SolarBodyOverride, World},
    },
    visual::{
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, SolarKeys},
        tentacle_grasp_carry::{CaptureCarry, CarryEvent, CarryPhase, CarryStatus, ReleaseReason},
        tentacle_grasp_preview::{ArmBuffers, GraspPreview, PreviewEvent, PreviewPhase},
        tentacle_live_pursuit::{LivePursuitRequest, PursuitMode},
        tentacle_solar_game::TentacleCaptureFlags,
    },
};

/// Tecla de DIAGNOSTICO (emergencia) heredada: suelta o reinicia; no es una mecanica de juego.
pub const EMERGENCY_KEY: &str = "X";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPhase {
    /// Vuelo libre con la persecucion existente.
    Pursuit,
    /// Nave atrapada en el contacto; gesto aprobado B1/P1 en curso (cuerpo inmovil declarado).
    Closing,
    /// Bloqueo aprobado: agarre autorado.
    Captured,
    /// Arrastre aprobado hacia la criatura.
    Dragging,
    /// Retenida junto a la criatura.
    Held,
    /// Ultima malla como observacion, nave libre (O2 recuperado, gesto fallido o bloqueo rechazado).
    Observation,
    /// Sin capa propia y nave libre (escena invalida).
    Aborted,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReleaseCause {
    /// O2 mostrado por encima del umbral de persecucion (la regla de O2 existente).
    OxygenRecovered,
    /// El gesto B1/P1 se detuvo (cancelacion u error del nucleo) antes del bloqueo.
    GestureStopped(String),
    /// El bloqueo del arrastre se rechazo en el Gripped.
    LockRejected(String),
    /// X de diagnostico.
    Emergency,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutoEvent {
    /// Contacto real: nave capturada en su pose, sin velocidad ni inercia.
    Contact,
    /// Contacto real que B1 rechazo (ancla, enlace o capa): nada se capturo.
    ContactRejected(String),
    /// Gripped: bloqueo del arrastre aprobado.
    Locked,
    DragStarted,
    HeldReached,
    Released(ReleaseCause),
    /// La persecucion volvio al reposo (re-armado sin salto visible o X).
    Rearmed,
    /// El reinicio pedido se rechazo (el World no tiene el reposo del clip).
    RearmRejected(String),
    Aborted(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutoStatus {
    pub phase: AutoPhase,
    pub events: Vec<AutoEvent>,
    /// Cuadros procesados con exito.
    pub frame: u64,
    /// `frame` del ultimo contacto capturado.
    pub contact_frame: Option<u64>,
    /// Pose de la nave capturada en el contacto (posicion, orientacion de `Ship`).
    pub contact_pose: Option<(Vec3, Mat4)>,
    /// Seguimiento corporal inmovilizado en este cuadro (cierre B1/P1).
    pub body_held: bool,
    /// Estado de la cadena aprobada (preview C + arrastre).
    pub carry: CarryStatus,
}

/// Nave libre con la MISMA pose y sin inercia (`Ship::new` + orientacion y ajustes); infalible.
fn free_ship(world: &mut World) {
    let mut ship = Ship::new(world.ship.position, -Vec3::Z);
    ship.orientation = world.ship.orientation;
    ship.settings = world.ship.settings;
    world.ship = ship;
    world.gameplay.navigation.gravity_velocity = Vec3::ZERO;
    world.gameplay.navigation.assisted_planet = None;
}

/// Captura automatica: posee la cadena aprobada (preview C dentro del arrastre, siempre habilitado).
pub struct AutoCapture<'c, 'p> {
    carry: CaptureCarry<'c, 'p>,
    /// Pose de contacto re-publicada durante el cierre B1/P1.
    pin: Option<(Vec3, Mat4)>,
    /// El episodio de la preview sigue vivo pero la nave ya se solto (bloqueo rechazado).
    detached: bool,
    status: AutoStatus,
    restart_generation: u64,
}

impl<'c, 'p> AutoCapture<'c, 'p> {
    /// Envuelve una preview YA enlazada (`GraspPreview::new`) con el arrastre aprobado habilitado.
    pub fn new(preview: GraspPreview<'c, 'p>) -> Result<Self, String> {
        let carry = CaptureCarry::new(preview, true)?;
        let status = AutoStatus {
            phase: AutoPhase::Pursuit,
            events: Vec::new(),
            frame: 0,
            contact_frame: None,
            contact_pose: None,
            body_held: false,
            carry: carry.status(),
        };
        Ok(Self {
            carry,
            pin: None,
            detached: false,
            status,
            restart_generation: 0,
        })
    }

    /// Vuelo (y vistas que recolocan la nave) bloqueado: desde el contacto hasta soltar.
    pub fn flight_locked(&self) -> bool {
        self.pin.is_some() || self.carry.locks_flight()
    }
    /// Seguimiento corporal inmovilizado (solo durante el cierre B1/P1).
    pub fn holds_body(&self) -> bool {
        self.pin.is_some()
    }

    /// Un cuadro DESPUES de la fisica del World (`dt` en `[0, MAX_PURSUIT_DT]`, el de la persecucion).
    /// `emergency` es la X de diagnostico. Con `Err` el adaptador y la cadena no cambian (la nave del cierre
    /// sigue en su pose de contacto); el escape ante una escena perdida es `abort`.
    pub fn frame(
        &mut self,
        world: &mut World,
        dt: f64,
        emergency: bool,
    ) -> Result<AutoStatus, String> {
        self.frame_owned(world, dt, emergency, false)
    }

    fn frame_owned(
        &mut self,
        world: &mut World,
        dt: f64,
        emergency: bool,
        defer_oxygen: bool,
    ) -> Result<AutoStatus, String> {
        if world.gameplay.game_over || world.gameplay.objective_complete {
            self.carry.validate_frozen(world, dt)?;
            return Ok(self.status());
        }
        if self.restart_generation != world.restart_generation {
            self.carry.validate_frozen(world, dt)?;
            self.restart_episode(world)?;
        }
        // La cadena rechaza dt/identidad sin mutarse. El pin previo tambien debe
        // ser transaccional: restaurar TODOS los campos de nave y camara en Err.
        let prior_ship = world.ship;
        let prior_camera = world.camera;
        let prior_navigation = world.gameplay.navigation.clone();
        if let Some((position, orientation)) = self.pin {
            // Referencia cinematica del cierre: la pose capturada EXACTA (la fisica neutra no la cambia).
            world.commit_captured_ship_pose(position, orientation)?;
        }
        let observing = matches!(
            self.status.phase,
            AutoPhase::Observation | AutoPhase::Aborted
        );
        let hidden = world.solar_reveal_alphas().is_some_and(|a| a.eye <= 0.0);
        let rearm = observing && hidden && !emergency;
        let st =
            match self
                .carry
                .frame_on_contact_owned(world, dt, emergency || rearm, defer_oxygen)
            {
                Ok(st) => st,
                Err(error) => {
                    world.ship = prior_ship;
                    world.camera = prior_camera;
                    world.gameplay.navigation = prior_navigation;
                    return Err(error);
                }
            };
        let mut events = Vec::new();
        for e in &st.events {
            match e {
                CarryEvent::Preview(PreviewEvent::Captured) => {
                    // Contacto real de ESTE cuadro: la nave queda donde ESTA y pierde velocidad e inercia.
                    let pose = (world.ship.position, world.ship.orientation);
                    match world.commit_captured_ship_pose(pose.0, pose.1) {
                        Ok(()) => {
                            self.pin = Some(pose);
                            self.detached = false;
                            self.status.contact_frame = Some(self.status.frame);
                            self.status.contact_pose = Some(pose);
                            events.push(AutoEvent::Contact);
                        }
                        Err(why) => {
                            let why = format!("contacto sin commit de la nave: {why}");
                            self.carry.abort(world, &why);
                            events.push(AutoEvent::Aborted(why));
                        }
                    }
                }
                CarryEvent::Preview(PreviewEvent::RequestRejected(why)) => {
                    events.push(AutoEvent::ContactRejected(why.clone()))
                }
                CarryEvent::Preview(PreviewEvent::Cancelled(reason)) => {
                    if self.release_pin(world) {
                        events.push(AutoEvent::Released(match reason {
                            crate::visual::tentacle_grasp_preview::CancelReason::OxygenAboveEngage => {
                                ReleaseCause::OxygenRecovered
                            }
                            other => ReleaseCause::GestureStopped(format!("{other:?}")),
                        }));
                    }
                }
                CarryEvent::Preview(PreviewEvent::Failed(why)) => {
                    if self.release_pin(world) {
                        events.push(AutoEvent::Released(ReleaseCause::GestureStopped(
                            why.clone(),
                        )));
                    }
                }
                CarryEvent::Preview(PreviewEvent::Reset) => {
                    let was = self.release_pin(world);
                    self.detached = false;
                    if was || emergency {
                        events.push(AutoEvent::Released(ReleaseCause::Emergency));
                    }
                    events.push(AutoEvent::Rearmed);
                }
                CarryEvent::Preview(PreviewEvent::ResetRejected(why))
                | CarryEvent::ResetRejected(why) => {
                    if self.release_pin(world) {
                        events.push(AutoEvent::Released(ReleaseCause::Emergency));
                    }
                    events.push(AutoEvent::RearmRejected(why.clone()));
                }
                CarryEvent::Locked => {
                    // El arrastre aprobado es ahora el dueno de la nave (su commit) y el cuerpo se reanuda.
                    self.pin = None;
                    events.push(AutoEvent::Locked);
                }
                CarryEvent::CarryRejected(why) => {
                    if self.release_pin(world) {
                        self.detached = true;
                        events.push(AutoEvent::Released(ReleaseCause::LockRejected(why.clone())));
                    }
                }
                CarryEvent::DragStarted => events.push(AutoEvent::DragStarted),
                CarryEvent::HeldReached => events.push(AutoEvent::HeldReached),
                CarryEvent::Released(ReleaseReason::OxygenAboveEngage) => {
                    events.push(AutoEvent::Released(ReleaseCause::OxygenRecovered))
                }
                CarryEvent::Released(ReleaseReason::ResetKey) => {
                    events.push(AutoEvent::Released(ReleaseCause::Emergency))
                }
                CarryEvent::Aborted(why) => events.push(AutoEvent::Aborted(why.clone())),
                _ => {}
            }
        }
        self.status.phase = self.phase();
        if events.iter().any(|e| matches!(e, AutoEvent::Released(_))) {
            free_ship(world);
        }
        self.status.events = events;
        self.status.body_held = self.pin.is_some();
        self.status.carry = st;
        self.status.frame += 1;
        Ok(self.status.clone())
    }

    fn restart_episode(&mut self, world: &mut World) -> Result<(), String> {
        let (azimuth, distance, height) = crate::visual::solar_game::SolarView::Frontal.profile();
        world.place_solar_reveal_camera(azimuth, distance, height)?;
        self.carry.restart_game(world)?;
        self.pin = None;
        self.detached = false;
        self.status = AutoStatus {
            phase: AutoPhase::Pursuit,
            events: Vec::new(),
            frame: 0,
            contact_frame: None,
            contact_pose: None,
            body_held: false,
            carry: self.carry.status(),
        };
        self.restart_generation = world.restart_generation;
        Ok(())
    }

    /// Reaccion unica posterior a la recogida autoritativa, sin cuadro sintetico.
    fn react_to_oxygen(&mut self, world: &mut World) -> Result<(), String> {
        if self.flight_locked() && world.debug_oxygen01 > 0.15 {
            self.carry.react_to_oxygen(world)?;
            self.release_pin(world);
            free_ship(world);
            self.status.phase = self.phase();
            self.status.body_held = false;
            self.status.carry = self.carry.status();
            self.status
                .events
                .push(AutoEvent::Released(ReleaseCause::OxygenRecovered));
        }
        Ok(())
    }

    /// Suelta la nave del cierre (misma pose, sin inercia); `false` si no estaba atrapada por el cierre.
    fn release_pin(&mut self, world: &mut World) -> bool {
        if self.pin.take().is_some() {
            free_ship(world);
            return true;
        }
        false
    }

    /// Escape EXPLICITO ante escena perdida o cuadro imposible: nave libre (misma pose, sin inercia), sin capa
    /// propia ni episodio dibujado. Infalible.
    pub fn abort(&mut self, world: &mut World, reason: &str) {
        self.release_pin(world);
        self.carry.abort(world, reason);
        self.status.phase = AutoPhase::Aborted;
        self.status.body_held = false;
        self.status.events = vec![AutoEvent::Aborted(reason.to_string())];
        self.status.carry = self.carry.status();
    }

    fn phase(&self) -> AutoPhase {
        match self.carry.status().phase {
            CarryPhase::Captured => AutoPhase::Captured,
            CarryPhase::Dragging => AutoPhase::Dragging,
            CarryPhase::Held => AutoPhase::Held,
            CarryPhase::Released => AutoPhase::Observation,
            CarryPhase::Aborted => AutoPhase::Aborted,
            CarryPhase::Free(PreviewPhase::Pursuit) => AutoPhase::Pursuit,
            CarryPhase::Free(PreviewPhase::Cancelled | PreviewPhase::Failed) => {
                AutoPhase::Observation
            }
            CarryPhase::Free(_) if self.pin.is_some() => AutoPhase::Closing,
            // Episodio vivo con la nave ya soltada (bloqueo rechazado): observacion del hold de C.
            CarryPhase::Free(_) => AutoPhase::Observation,
        }
    }

    pub fn status(&self) -> AutoStatus {
        self.status.clone()
    }
    pub fn carry(&self) -> &CaptureCarry<'c, 'p> {
        &self.carry
    }
    /// Capa `Body` dibujada (persecucion, episodio, bloqueo u observacion; abortada: ninguna).
    pub fn body_override(&self) -> Option<SolarBodyOverride<'_>> {
        self.carry.body_override()
    }
    /// Tramo dibujado (vertices, normales) en OBJ de la criatura.
    pub fn arm(&self) -> Option<ArmBuffers<'_>> {
        self.carry.arm()
    }
    /// Distancia (OBJ) del centroide grip de la malla dibujada al ancla aprobada con la nave y el cuerpo
    /// ACTUALES del World (persecucion: la del episodio de C; sin episodio: `None`).
    pub fn final_mesh_contact_error(&self, world: &World) -> Option<f64> {
        self.carry.final_mesh_contact_error(world)
    }
    /// Titulo de la ventana (sin HUD).
    pub fn title(&self) -> String {
        let s = &self.status;
        let c = &s.carry;
        match s.phase {
            AutoPhase::Pursuit => {
                let pursuit = self.carry.preview().pursuit();
                let mode = pursuit
                    .last()
                    .map(|p| match p.step.mode {
                        PursuitMode::Idle => "TENTACULO en reposo (O2 > 15 %)",
                        PursuitMode::Pursuing => "TENTACULO PERSIGUE a la nave",
                        PursuitMode::Retracting => "TENTACULO se recoge",
                    })
                    .unwrap_or("TENTACULO sin paso");
                let distance = pursuit.last().map_or(String::new(), |p| {
                    format!(" | punta-nave {:.0} u", p.tip_ship_distance)
                });
                format!("{mode}{distance}")
            }
            AutoPhase::Closing => {
                let p = &c.preview;
                let what = match p.phase {
                    PreviewPhase::Handoff => format!(
                        "traspaso {:.2}/{:.2} s",
                        p.handoff.map_or(0.0, |h| h.elapsed_s),
                        p.handoff.map_or(0.0, |h| h.duration_s)
                    ),
                    _ => format!(
                        "cierre cuadro {}/{}",
                        p.closure.map_or(0, |c| c.frame),
                        self.carry.preview().clip().grip_frame()
                    ),
                };
                format!("NAVE ATRAPADA: el tentaculo la envuelve ({what}) | vuelo bloqueado")
            }
            AutoPhase::Captured => format!(
                "NAVE CAPTURADA: agarre {:.2}/{:.2} s | vuelo bloqueado",
                c.elapsed_s,
                self.carry.plan().hold_s
            ),
            AutoPhase::Dragging => format!(
                "ARRASTRE hacia la criatura {:.0} % | vuelo bloqueado",
                c.progress * 100.0
            ),
            AutoPhase::Held => "RETENIDA junto a la criatura | vuelo bloqueado".to_string(),
            AutoPhase::Observation => {
                "SUELTA: nave libre; el tentaculo queda quieto hasta ocultarse la criatura"
                    .to_string()
            }
            AutoPhase::Aborted => format!(
                "TENTACULO ABORTADO: {} | nave libre",
                c.error.as_deref().unwrap_or("?")
            ),
        }
    }
}

/// Controles ya leidos de un cuadro (ventana, replay y pruebas).
#[derive(Clone, Copy, Debug, Default)]
pub struct AutoControls {
    pub ship: ShipInput,
    pub oxygen: OxygenKeys,
    /// R (reset), 7, 8, H, G de la ruta solar.
    pub solar: SolarKeys,
    /// X de diagnostico.
    pub emergency: bool,
}

#[derive(Clone, Debug)]
pub struct AutoFrame {
    /// Entrada que recibio el World (neutra con la nave capturada).
    pub effective_ship: ShipInput,
    /// Teclas solares aplicadas (sin R/7/8 con la nave capturada).
    pub solar_applied: SolarKeys,
    /// Se pidio una vista que recoloca la nave con la nave capturada y se ignoro.
    pub solar_blocked: bool,
    /// El cuadro avanzo con el seguimiento corporal inmovilizado (cierre B1/P1).
    pub body_held: bool,
    pub status: Result<AutoStatus, String>,
    /// El cuadro fallo con la nave capturada o con capa propia y se ejecuto `abort`.
    pub aborted: bool,
}

/// Cuadro COMPARTIDO por el bucle principal (ventana y replay) y las pruebas: preflight numerico (el mismo
/// RUNTIME01 aprobado) -> teclas solares (sin R/7/8 con la nave capturada) -> World con la entrada EFECTIVA
/// y los segundos reales (O2 incluido; cuerpo inmovil durante el cierre) -> captura automatica con
/// `dt = min(elapsed, MAX_PHYSICS_DT)` -> render (lo hace el llamador). Si el cuadro falla con la nave
/// capturada o con capa propia, `abort` explicito (nave libre, sin capa).
pub fn advance_auto_capture_frame(
    world: &mut World,
    game: &mut SolarGame,
    oxygen: &mut OxygenTransition,
    auto: &mut AutoCapture,
    controls: AutoControls,
    elapsed: f64,
) -> Result<AutoFrame, String> {
    // Dominio numerico ANTES de H/G/R o cualquier otra mutacion (copia del preflight aprobado RUNTIME01).
    let mut oxygen_candidate = *oxygen;
    oxygen_candidate.press(controls.oxygen)?;
    oxygen_candidate.advance(elapsed)?;
    let raw = controls.ship;
    if ![raw.thrust, raw.yaw, raw.pitch, raw.roll]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err("entrada numerica de vuelo no finita; nada cambia".into());
    }
    let mut ship_candidate = world.ship;
    ship_candidate.update(&raw, elapsed.min(MAX_PHYSICS_DT));
    let render = ship_candidate.transform(SHIP_MODEL_YAW).matrix();
    let composed = world.camera.view_projection() * render;
    let axes = [0, 1, 2].map(|c| ship_candidate.orientation.column(c));
    if !(ship_candidate.position.is_finite()
        && ship_candidate.speed.is_finite()
        && axes
            .iter()
            .all(|a| a.is_finite() && (a.length() - 1.0).abs() <= 1e-9)
        && render.m.iter().flatten().all(|v| v.is_finite())
        && composed.m.iter().flatten().all(|v| v.is_finite())
        && world.ship_model.vertices.iter().all(|&v| {
            let clip = composed.transform(Vec3::from_array(v).extend(1.0));
            [clip.x, clip.y, clip.z, clip.w]
                .iter()
                .all(|v| v.is_finite())
        }))
    {
        return Err("composicion numerica del vuelo no finita o degenerada; nada cambia".into());
    }
    let frozen_dt = elapsed.min(MAX_PHYSICS_DT);
    // CORRECTION05: identidad EXACTA (inmutable) ANTES de reinicio, teclas solares, tick, O2 o captura: el tick
    // puede alcanzar el terminal y los retornos `frozen` posteriores no deben aceptar una criatura PRESENTE ajena.
    // Sin criatura (escena perdida) se conserva el escape aprobado: `abort` explicito sin permalock.
    if world.creature().is_some() {
        auto.carry.validate_frozen(world, frozen_dt)?;
    }
    if auto.restart_generation != world.restart_generation {
        // CORRECTION03: la MISMA identidad exacta y preflight que `frame_owned` ANTES de cualquier
        // mutacion; un World con criatura ajena no se reenlaza en silencio (nada cambia en Err).
        auto.carry.validate_frozen(world, frozen_dt)?;
        // New game is not X/release: discard the old contact and reset the accepted chain.
        game.apply(
            world,
            SolarKeys {
                reset: true,
                ..SolarKeys::default()
            },
        )?;
        auto.restart_episode(world)?;
        *oxygen = OxygenTransition::new(world.gameplay.oxygen01)?;
    }
    let locked = auto.flight_locked();
    let frozen = |auto: &AutoCapture| AutoFrame {
        effective_ship: ShipInput::default(),
        solar_applied: SolarKeys::default(),
        solar_blocked: controls.solar.reset || controls.solar.frontal || controls.solar.oblique,
        body_held: auto.holds_body(),
        status: Ok(auto.status()),
        aborted: false,
    };
    if world.gameplay.game_over || world.gameplay.objective_complete {
        // CORRECTION03: terminal (tambien antes del primer cuadro) congela solo un World ENLAZADO.
        auto.carry.validate_frozen(world, frozen_dt)?;
        return Ok(frozen(auto));
    }
    let mut solar = controls.solar;
    let solar_blocked = locked && (solar.reset || solar.frontal || solar.oblique);
    if locked {
        (solar.reset, solar.frontal, solar.oblique) = (false, false, false);
    }
    let effective_ship = if locked {
        ShipInput::default()
    } else {
        controls.ship
    };
    game.apply(world, solar)?;
    let body_held = auto.holds_body();
    let path_start = world.ship.position;
    let path_time = world.time;
    if locked {
        world.update_with_pose_owner(&effective_ship, elapsed.min(MAX_PHYSICS_DT), true);
        if world.gameplay.game_over || world.gameplay.objective_complete {
            return Ok(frozen(auto));
        }
        // Approved coupling: body follows the current pre-carry target ONCE, then carry
        // composes against that body. This step does not read oxygen. Never move body again
        // after the authoritative ship commit (which would break the rigid grip).
        if body_held {
            world.update_creature_eye_follow_body_held(elapsed);
        } else {
            world.update_creature_eye_follow(elapsed);
        }
    } else {
        advance_perception_frame_owned(
            world,
            oxygen,
            controls.oxygen,
            &effective_ship,
            elapsed,
            false,
            false,
        )?;
    }
    if world.gameplay.game_over || world.gameplay.objective_complete {
        return Ok(frozen(auto));
    }
    // A recovery already caused by the command/consumption must release at the old pose
    // (the approved no-teleport contract). A pickup-caused recovery can only be decided
    // AFTER the authoritative carry path. This probe has no publication or side effects.
    let transition = |world: &World| -> Result<OxygenTransition, String> {
        let mut next = *oxygen;
        next.press(controls.oxygen)?;
        let override_value = if controls.oxygen.preset.is_some()
            || controls.oxygen.increase
            || controls.oxygen.decrease
        {
            Some(next.target())
        } else {
            world.forced_oxygen
        };
        next.press(OxygenKeys {
            preset: Some(override_value.unwrap_or(world.gameplay.oxygen01)),
            ..OxygenKeys::default()
        })?;
        next.advance(elapsed)?;
        Ok(next)
    };
    let recovery_known = locked && transition(world)?.shown() > 0.15;
    let mut status;
    if recovery_known {
        // No carry integration can occur: the FINAL shown value is already above threshold.
        // The stationary path is authoritative, not a provisional second collection.
        world
            .gameplay
            .collect_along(path_start, path_start, &world.system, path_time, world.time);
        let next = transition(world)?;
        world.publish_visual_oxygen(next.shown());
        if controls.oxygen.preset.is_some() || controls.oxygen.increase || controls.oxygen.decrease
        {
            world.forced_oxygen = Some(next.target());
        }
        *oxygen = next;
        if world.gameplay.objective_complete {
            return Ok(frozen(auto));
        }
        status = auto.frame_owned(
            world,
            elapsed.min(MAX_PHYSICS_DT),
            controls.emergency,
            false,
        );
    } else {
        status = auto.frame_owned(
            world,
            elapsed.min(MAX_PHYSICS_DT),
            controls.emergency,
            locked,
        );
        if locked && status.is_ok() {
            world.gameplay.collect_along(
                path_start,
                world.ship.position,
                &world.system,
                path_time,
                world.time,
            );
            let next = transition(world)?;
            world.publish_visual_oxygen(next.shown());
            if controls.oxygen.preset.is_some()
                || controls.oxygen.increase
                || controls.oxygen.decrease
            {
                world.forced_oxygen = Some(next.target());
            }
            *oxygen = next;
            if !world.gameplay.objective_complete {
                auto.react_to_oxygen(world)?;
            }
            status = Ok(auto.status());
        }
    }
    if locked && status.is_ok() && !world.gameplay.objective_complete {
        world.update_creature_reveal(elapsed);
    }
    let aborted = status.is_err() && (auto.flight_locked() || auto.carry().has_carry_layer());
    if let (true, Err(e)) = (aborted, &status) {
        auto.abort(world, e);
    }
    Ok(AutoFrame {
        effective_ship,
        solar_applied: solar,
        solar_blocked,
        body_held,
        status,
        aborted,
    })
}

/// Combinaciones de `--tentacle-auto-capture` (y `--auto-capture-replay`) sin tocar archivos. Sin el flag:
/// `Ok(None)` (y el replay se rechaza); las demas rutas del tentaculo siguen sus reglas de siempre.
pub fn validate_auto_capture_flags(
    auto: bool,
    replay: bool,
    spatial: bool,
    live: bool,
    capture: &TentacleCaptureFlags,
) -> Result<Option<LivePursuitRequest>, String> {
    if !auto {
        if replay {
            return Err("--auto-capture-replay requiere --tentacle-auto-capture".into());
        }
        return Ok(None);
    }
    if spatial || live {
        return Err("--tentacle-auto-capture ya incluye la persecucion espacial: no se combina con --tentacle-spatial-pursuit ni --tentacle-live-pursuit".into());
    }
    if capture.enabled {
        return Err("--tentacle-auto-capture y --tentacle-capture-debug son incompatibles".into());
    }
    if capture.start_time.is_some() {
        return Err("--tentacle-auto-capture no admite --tentacle-capture-time".into());
    }
    if capture.legacy {
        return Err("--legacy no admite --tentacle-auto-capture".into());
    }
    if !capture.solar {
        return Err(
            "--tentacle-auto-capture requiere --creature-solar-reveal (captura en la ruta solar)"
                .into(),
        );
    }
    let need = |v: &Option<std::path::PathBuf>, flag: &str| {
        v.clone()
            .ok_or(format!("--tentacle-auto-capture requiere {flag}"))
    };
    Ok(Some(LivePursuitRequest {
        capture_dir: need(
            &capture.capture_dir,
            "--tentacle-capture-package <carpeta capture_v2>",
        )?,
        growth_package: need(
            &capture.growth_package,
            "--tentacle-growth-package <growth_package.txt>",
        )?,
        correspondence: need(
            &capture.correspondence,
            "--tentacle-arm-correspondence <arm_correspondence.txt>",
        )?,
    }))
}

/// Ayuda minima (lanzador y `--help`).
pub fn help_text() -> &'static str {
    "CAPTURA AUTOMATICA del tentaculo (--tentacle-auto-capture, opt-in; requiere --creature-solar-reveal y --lucid-perception o --watching-stars, y los paquetes --tentacle-capture-package, --tentacle-growth-package y --tentacle-arm-correspondence). Arranca al 100 % de O2: el tentaculo reposa. Con O2 <= 15 % persigue a la nave (persecucion espacial existente). Si la punta ALCANZA la nave, la nave queda atrapada en ese mismo cuadro donde esta (sin T ni quietud): el vuelo y R/7/8 se ignoran, el tentaculo la envuelve (gesto aprobado) y la arrastra hacia la criatura. Subir el O2 por encima del 15 % la suelta sin inercia. X: escape de diagnostico (suelta/reinicia)."
}
