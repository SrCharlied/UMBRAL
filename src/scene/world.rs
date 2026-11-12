//! Mundo de juego: avanza la simulacion con `dt` y dibuja un cuadro completo.

use std::cell::{Cell, RefCell};

use super::{
    camera::Camera,
    sectors::SectorField,
    ship::{Ship, ShipInput, smoothing},
    solar_system::{CelestialBody, SolarSystem},
};
use crate::{
    assets::{
        obj::{Face, Model},
        primitives::uv_sphere,
    },
    math::{Mat4, Transform, Vec3},
    renderer::{
        clipping::Frustum,
        framebuffer::Framebuffer,
        pipeline::draw_model,
        shader::{FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment, to_rgb8},
        stats::RenderStats,
    },
    visual::{
        creature_distant_turn::{
            CreaturePreset, DistantTurn, DistantTurnStatus, TurnAngles, aim_turn, distance_weight,
            preset_pose, rest_frame_target, turn_off_axis, turned_transform,
        },
        creature_eye_comparison::{EyeComparisonStatus, EyeTarget, off_axis_degrees},
        creature_eye_follow::{EyeFollow, EyeFollowSettings, EyePoseCache, EyeSkin, GazeAngles},
        creature_horizontal_follow::{
            HorizontalFollow, HorizontalFollowStatus, horizontal_transform, rest_frame_point,
            target_azimuth,
        },
        creature_reveal::{CreatureReveal, RevealCompositor, RevealPart, RevealSettings},
        planet_props::{PlanetProps, build_system_props, oxygen_tank_mesh, prop_light_position},
        solar_reveal_preview::{
            PairLayer, PairLayerKind, SolarPairCompositor, SolarRevealAlphas, SolarRevealLight,
            solar_reveal_alphas,
        },
        staged_creature_reveal::{EyeRegion, StagedParts, StagedReveal, StagedRevealSettings},
        static_creature::{AMBIENT, MAX_WORLD_COORD, StaticCreature},
        textured::{TexturedMode, TexturedShader},
    },
};

/// Anillos x segmentos por nivel de detalle, del mas fino al mas grueso.
pub const LOD_RESOLUTIONS: [(usize, usize); 3] = [(32, 64), (16, 32), (8, 16)];
/// Tamano aparente minimo (radio / distancia) para usar cada nivel; el ultimo vale siempre.
pub const LOD_MIN_RATIO: [f64; 2] = [0.04, 0.01];
/// Margen relativo alrededor de cada umbral para evitar parpadeo.
pub const LOD_HYSTERESIS: f64 = 0.15;

/// Respuesta (1/s) con la que la orientacion de la camara alcanza la de la nave.
pub const CAMERA_TURN_RESPONSE: f64 = 6.0;
pub const BASE_FOV: f64 = 60.0 * std::f64::consts::PI / 180.0;
pub const BOOST_FOV: f64 = 75.0 * std::f64::consts::PI / 180.0;

pub const SPACE_COLOR: u32 = 0x03040a;
/// Distancia maxima de dibujo de planetas errantes y tanques de los sectores.
pub const SECTOR_DRAW_DISTANCE: f64 = 2400.0;
/// Duracion del aviso «+N% O2» tras recoger un tanque.
pub const PICKUP_FLASH_SECONDS: f64 = 2.0;
/// El OBJ de la nave apunta a +Z o -Z segun Blender; este giro lo alinea con -Z (D11).
pub const SHIP_MODEL_YAW: f64 = std::f64::consts::PI;

/// Interruptores de demostracion (UMB-404).
#[derive(Clone, Copy, Debug)]
pub struct DebugToggles {
    pub frustum_culling: bool,
    pub freeze_orbits: bool,
    pub lod: bool,
    pub backface_culling: bool,
    /// Props procedurales sobre los planetas (F2).
    pub planet_props: bool,
}

impl Default for DebugToggles {
    fn default() -> Self {
        Self {
            frustum_culling: true,
            freeze_orbits: false,
            lod: true,
            backface_culling: true,
            planet_props: true,
        }
    }
}

pub struct World {
    pub gameplay: crate::gameplay::GameplayState,
    pub orbit_assist: bool,
    pub forced_oxygen: Option<f64>,
    mirrored_oxygen: f64,
    visual_transition_owned: bool,
    pub(crate) restart_generation: u64,
    pub system: SolarSystem,
    pub ship: Ship,
    pub camera: Camera,
    pub ship_model: Model,
    /// Esferas de mas a menos detalle; ver `LOD_MIN_RATIO` y `World::lod_level`.
    pub sphere_lods: Vec<Model>,
    /// Props procedurales por cuerpo con sus anclajes persistentes (`visual::planet_props`).
    pub planet_props: Vec<Option<PlanetProps>>,
    /// Ultimo nivel elegido por cuerpo; da histeresis para que no parpadee en el limite.
    body_lod: Vec<Cell<usize>>,
    /// Segundos simulados; avanza con `dt` salvo que las orbitas esten congeladas.
    pub time: f64,
    pub toggles: DebugToggles,
    /// Oxigeno de depuracion hasta que Persona 2 entregue `GameplayState`.
    /// TODO(UMB-205): sustituir por el valor de `GameplayState`.
    pub debug_oxygen01: f64,
    /// Orientacion suavizada con la que la camara sigue a la nave.
    camera_basis: Mat4,
    pub chase_distance: f64,
    pub chase_height: f64,
    /// Criatura estatica opcional (debug, opt-in); `None` en `World::new`.
    creature: Option<StaticCreature>,
    /// Revelacion de la criatura por oxigeno (debug, opt-in); `None`: siempre visible.
    creature_reveal: Option<CreatureReveal>,
    /// Buffers reutilizables de la revelacion parcial; `render` toma `&self`.
    reveal_compositor: RefCell<RevealCompositor>,
    /// Revelacion por etapas (debug, opt-in); excluyente con `creature_reveal`.
    staged_reveal: Option<StagedMode>,
    /// Sol con ojo (composicion aprobada 03, opt-in): solo lo dibuja
    /// `render_solar_reveal_preview` (el juego con `--creature-solar-reveal`).
    solar_preview: Option<SolarPreview>,
    /// Espacio procedural alrededor del sistema (opt-in, `enable_sectors`): planetas errantes y
    /// tanques de oxigeno por sectores. Con `Some`, los coleccionables usan el modelo de tanque.
    pub sectors: Option<SectorField>,
    /// Teclas de O2 de la percepcion (1-4, M/N) arrastran tambien el O2 REAL (presentacion).
    pub oxygen_keys_drive_real: bool,
    /// Segundos que le quedan al aviso de recogida de O2 (HUD de la presentacion).
    pub pickup_flash: f64,
    /// O2 que sumo la ultima recogida (fraccion), para el aviso.
    pub last_pickup_gain: f64,
    /// Secuencia de muerte por el monstruo en curso (`visual::monster_death`): el HUD muestra 0 %.
    pub dying: bool,
    /// Multiplicador del consumo de O2 con el turbo (Shift); 1 = sin castigo. La experiencia por
    /// defecto usa `BOOST_OXYGEN_DRAIN`.
    pub boost_oxygen_drain: f64,
    /// Modelo del tanque de oxigeno (coleccionables con `sectors`).
    tank_model: Model,
}

/// Capa BODY alternativa del compositor solar (debug opt-in, `visual::tentacle_solar_game`): UNA
/// malla en espacio OBJ de la criatura = la particion `body` vigente (sus vertices, UV y normales como
/// PREFIJO exacto) con caras quitadas por procedencia verificada + un apendice (el brazo evaluado)
/// anadido despues. Se dibuja como la capa `Body` de siempre (mismo transform, material, shader,
/// textura y alfa del cuerpo); el ojo, el Sol y el compositor no cambian. `extra_bounds_obj` es la
/// esfera OBJ del apendice: el descarte la suma a la esfera original de la criatura (nunca recoloca
/// ni recalibra la criatura).
pub struct SolarBodyOverride<'a> {
    pub model: &'a Model,
    pub extra_bounds_obj: (Vec3, f64),
}

/// Preview solar: el cuerpo celeste que hace de Sol (su representacion se funde; el cuerpo,
/// las orbitas y la luz global siguen intactos), la luz local de la criatura y los buffers
/// reutilizables de la pareja central (`render_solar_reveal_preview` toma `&self`).
struct SolarPreview {
    sun: usize,
    light: SolarRevealLight,
    compositor: RefCell<SolarPairCompositor>,
}

/// Estado del modo por etapas: progreso, region del ojo y la particion en cache del modelo
/// de la criatura actual (`None` sin criatura).
struct StagedMode {
    reveal: StagedReveal,
    region: EyeRegion,
    parts: Option<StagedParts>,
    /// Mirada local (debug, opt-in); solo existe dentro del modo por etapas.
    eye: Option<EyeMode>,
}

/// Mirada local: estado angular ligero, el punto al que mira y, con criatura, su calibracion
/// y su pose en cache.
struct EyeMode {
    follow: EyeFollow,
    target: EyeTarget,
    initial_target: EyeTarget,
    rig: Option<EyeRig>,
    /// Giro global lejano (candidata, opt-in); solo existe con la mirada.
    turn: Option<DistantTurn>,
    /// Seguimiento corporal horizontal (candidata 02, opt-in); excluyente con `turn`.
    horizontal: Option<HorizontalFollow>,
}

/// Calibracion y geometria del ojo de la criatura actual, preparadas **una vez** (al activar
/// la mirada o en `set_creature`); `render` solo posa en sitio cuando cambia el angulo.
struct EyeRig {
    skin: EyeSkin,
    cache: RefCell<EyePoseCache>,
    /// Radio de descarte en mundo alrededor de `placement.center()`: la envolvente posada
    /// (`posed_bounds_radius`) escalada como el modelo, nunca menor que la esfera original.
    cull_radius: f64,
    /// Poses aplicadas (QA): solo crece al dibujar con un angulo distinto del de la cache.
    pose_updates: Cell<usize>,
}

impl EyeRig {
    fn new(creature: &StaticCreature, region: &EyeRegion) -> Result<Self, String> {
        let skin = EyeSkin::calibrate(creature.model(), region)?;
        let cache = EyePoseCache::new(creature.model(), &skin)?;
        let placement = creature.placement();
        let scaled = skin.posed_bounds_radius()
            * (placement.world_radius() / creature.model().bounds_radius);
        // Inalcanzable con los contratos de `CreatureBounds` y `EyeSkin` (radios finitos y
        // acotados); se comprueba igual para no descartar con un radio sin sentido.
        if !scaled.is_finite() {
            return Err(format!("radio de descarte del ojo no finito: {scaled}"));
        }
        Ok(Self {
            skin,
            cache: RefCell::new(cache),
            cull_radius: scaled.max(placement.world_radius()),
            pose_updates: Cell::new(0),
        })
    }
}

impl World {
    pub fn new(ship_model: Model, aspect: f64) -> Result<Self, String> {
        let ship = Self::start_ship();
        let body_lod = vec![Cell::new(0); SolarSystem::demo().bodies.len()];
        let mut world = Self {
            gameplay: crate::gameplay::GameplayState::default(),
            orbit_assist: false,
            forced_oxygen: None,
            mirrored_oxygen: 1.0,
            visual_transition_owned: false,
            restart_generation: 0,
            system: SolarSystem::demo(),
            planet_props: build_system_props(&SolarSystem::demo())?,
            ship,
            camera: Camera::new(BASE_FOV, aspect, 0.3, 6000.0)?,
            ship_model,
            body_lod,
            sphere_lods: LOD_RESOLUTIONS
                .iter()
                .map(|&(r, s)| uv_sphere(r, s))
                .collect(),
            time: 0.0,
            toggles: DebugToggles::default(),
            debug_oxygen01: 1.0,
            camera_basis: Mat4::IDENTITY,
            chase_distance: 22.0,
            chase_height: 6.0,
            creature: None,
            creature_reveal: None,
            reveal_compositor: RefCell::new(RevealCompositor::new()),
            staged_reveal: None,
            solar_preview: None,
            sectors: None,
            oxygen_keys_drive_real: false,
            pickup_flash: 0.0,
            last_pickup_gain: 0.0,
            dying: false,
            boost_oxygen_drain: 1.0,
            tank_model: oxygen_tank_mesh(),
        };
        world.snap_camera();
        Ok(world)
    }

    /// Nivel de detalle (0 = el mas fino) del cuerpo `index` segun su tamano aparente.
    /// Con histeresis: sube de nivel con un 15 % de margen sobre el umbral y baja con 15 % menos.
    pub fn lod_level(&self, index: usize, body_radius: f64, distance: f64) -> usize {
        if !self.toggles.lod {
            self.body_lod[index].set(0);
            return 0;
        }
        let ratio = body_radius / distance.max(1e-9);
        let mut level = self.body_lod[index].get();
        while level > 0 && ratio >= LOD_MIN_RATIO[level - 1] * (1.0 + LOD_HYSTERESIS) {
            level -= 1;
        }
        while level < LOD_MIN_RATIO.len() && ratio < LOD_MIN_RATIO[level] * (1.0 - LOD_HYSTERESIS) {
            level += 1;
        }
        self.body_lod[index].set(level);
        level
    }

    fn start_ship() -> Ship {
        Ship::new(Vec3::new(0.0, 40.0, 700.0), -Vec3::Z)
    }

    /// Restablece la nave; conserva el oxigeno y lleva la revelacion y la mirada a su objetivo.
    pub fn reset_ship(&mut self) {
        self.ship = Self::start_ship();
        self.gameplay.navigation.gravity_velocity = Vec3::ZERO;
        self.gameplay.navigation.assisted_planet = None;
        self.snap_camera();
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
    }

    /// Explicit new game, not visual relocation. Preserve configurable gameplay settings.
    pub fn restart_game(&mut self) {
        self.restart_generation = self.restart_generation.wrapping_add(1);
        let settings = self.gameplay.settings;
        self.gameplay = crate::gameplay::GameplayState::default();
        self.gameplay.settings = settings;
        if let Some(field) = self.sectors.as_mut() {
            field.reset_collected();
        }
        self.pickup_flash = 0.0;
        self.last_pickup_gain = 0.0;
        self.dying = false;
        self.time = 0.0;
        self.orbit_assist = false;
        self.forced_oxygen = None;
        self.debug_oxygen01 = 1.0;
        self.mirrored_oxygen = 1.0;
        self.visual_transition_owned = false;
        if let Some(eye) = self.staged_reveal.as_mut().and_then(|m| m.eye.as_mut()) {
            eye.target = eye.initial_target;
            if eye.turn.is_some() {
                eye.turn = Some(DistantTurn::new());
            }
            if eye.horizontal.is_some() {
                eye.horizontal = Some(HorizontalFollow::new());
            }
        }
        self.ship = Self::start_ship();
        self.snap_camera();
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
    }

    /// Oxigeno visual: override explicito, control legado, o valor real.
    pub fn oxygen01(&self) -> f64 {
        if self.visual_transition_owned {
            return self.debug_oxygen01;
        }
        let value = self.forced_oxygen.unwrap_or({
            if self.debug_oxygen01 != self.mirrored_oxygen {
                self.debug_oxygen01
            } else {
                self.gameplay.oxygen01
            }
        });
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            self.gameplay.oxygen01
        }
    }

    pub fn set_debug_oxygen(&mut self, value: f64) {
        if value.is_finite() {
            self.visual_transition_owned = false;
            self.forced_oxygen = Some(value.clamp(0.0, 1.0));
            self.debug_oxygen01 = value.clamp(0.0, 1.0);
            self.mirrored_oxygen = self.debug_oxygen01;
        }
    }

    pub fn clear_debug_oxygen(&mut self) {
        self.forced_oxygen = None;
        if !self.visual_transition_owned {
            self.debug_oxygen01 = self.gameplay.oxygen01;
            self.mirrored_oxygen = self.debug_oxygen01;
        }
    }

    /// Internal publication is not a debug command and never changes the real reserve.
    pub(crate) fn publish_visual_oxygen(&mut self, shown: f64) {
        self.debug_oxygen01 = shown;
        self.mirrored_oxygen = shown;
        self.visual_transition_owned = true;
    }

    /// Coloca la nave a `surface_gap` unidades de la superficie del cuerpo `index`,
    /// mirando hacia el. Util para demostrar el acercamiento sin volar desde el inicio.
    pub fn place_near(&mut self, index: usize, surface_gap: f64) -> Result<(), String> {
        let body = self.system.bodies.get(index).ok_or("Cuerpo inexistente")?;
        let center = body.position(self.time);
        let radial = Vec3::new(center.x, 0.0, center.z).normalized();
        let radial = if radial == Vec3::ZERO {
            Vec3::Z
        } else {
            radial
        };
        // Lado tangencial: ve al cuerpo medio iluminado por el Sol.
        let side = Vec3::Y.cross(radial);
        let eye = center
            + (side * 0.8 + radial * 0.3).normalized() * (body.body_radius + surface_gap)
            + Vec3::Y * (body.body_radius * 0.15);
        self.ship = Ship::new(eye, (center - eye).normalized());
        self.snap_camera();
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
        Ok(())
    }

    /// Posicion y objetivo de la camara: pegada a la nave (sin retraso de posicion) pero
    /// orientada con `camera_basis`, que sigue a la nave con suavizado. Asi la distancia
    /// camara-nave es constante a cualquier velocidad y solo la orientacion "se arrastra".
    fn desired_camera(&self) -> (Vec3, Vec3) {
        if self.orbit_assist
            && let Some(body) = self
                .gameplay
                .navigation
                .assisted_planet
                .and_then(|index| self.system.bodies.get(index))
        {
            let center = body.position(self.time);
            let radial = (self.ship.position - center).normalized();
            // Navigation conserva este eje perpendicular al plano orbital, incluso en polos.
            let up = self.ship.up();
            return (
                self.ship.position + radial * self.chase_distance + up * self.chase_height,
                center,
            );
        }
        let basis = &self.camera_basis;
        let back = basis.transform_dir(Vec3::Z);
        let up = basis.transform_dir(Vec3::Y);
        let position = self.ship.position + back * self.chase_distance + up * self.chase_height;
        (position, self.ship.position - back * 30.0)
    }

    fn desired_camera_up(&self) -> Vec3 {
        if self.orbit_assist && self.gameplay.navigation.assisted_planet.is_some() {
            self.ship.up()
        } else {
            self.camera_basis.transform_dir(Vec3::Y)
        }
    }

    fn snap_camera(&mut self) {
        self.camera_basis = self.ship.orientation;
        let (position, target) = self.desired_camera();
        self.camera.position = position;
        self.camera.target = target;
        self.camera.up = self.desired_camera_up();
    }

    pub fn update(&mut self, input: &ShipInput, dt: f64) {
        self.update_with_pose_owner(input, dt, false);
    }

    pub(crate) fn update_with_pose_owner(&mut self, input: &ShipInput, dt: f64, captured: bool) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        if self.gameplay.game_over || self.gameplay.objective_complete {
            return;
        }
        if self.debug_oxygen01 != self.mirrored_oxygen && self.debug_oxygen01.is_finite() {
            self.forced_oxygen = Some(self.debug_oxygen01.clamp(0.0, 1.0));
        }
        self.gameplay.tick(dt);
        self.debug_oxygen01 = self.oxygen01();
        self.mirrored_oxygen = self.debug_oxygen01;
        if self.gameplay.game_over {
            return;
        }
        let old_time = self.time;
        if !self.toggles.freeze_orbits {
            self.time += dt;
        }
        if captured {
            // Capture publishes the only pose; never visit provisional physics or pickups.
            self.gameplay.navigation.gravity_velocity = Vec3::ZERO;
            self.gameplay.navigation.assisted_planet = None;
            return;
        }
        let previous = self.ship.position;
        let (picked_before, oxygen_before) =
            (self.gameplay.collected_resources, self.gameplay.oxygen01);
        self.pickup_flash = (self.pickup_flash - dt).max(0.0);
        self.ship.update(input, dt);
        self.gameplay
            .navigation
            .apply_gravity(&mut self.ship, &self.system, self.time, dt);
        self.gameplay.navigation.assist(
            &mut self.ship,
            input,
            &self.system,
            old_time,
            self.time,
            dt,
            self.orbit_assist,
        );
        self.gameplay.navigation.resolve_collisions(
            &mut self.ship,
            previous,
            &self.system,
            old_time,
            self.time,
        );
        self.gameplay.collect_along(
            previous,
            self.ship.position,
            &self.system,
            old_time,
            self.time,
        );
        self.update_sectors(previous);
        self.drain_boost_oxygen(dt);
        if self.gameplay.collected_resources > picked_before {
            self.pickup_flash = PICKUP_FLASH_SECONDS;
            self.last_pickup_gain = (self.gameplay.oxygen01 - oxygen_before).max(0.0);
        }
        if !self.visual_transition_owned {
            self.debug_oxygen01 = self.forced_oxygen.unwrap_or(self.gameplay.oxygen01);
            self.mirrored_oxygen = self.debug_oxygen01;
        }
        // La base de la camara persigue la orientacion de la nave con suavizado
        // independiente de los FPS; mezclar y reortonormalizar es estable para pasos chicos.
        let k = smoothing(CAMERA_TURN_RESPONSE, dt);
        let mut blended = self.camera_basis;
        for (row, target_row) in blended.m.iter_mut().zip(self.ship.orientation.m) {
            for (value, target) in row.iter_mut().zip(target_row) {
                *value += (target - *value) * k;
            }
        }
        self.camera_basis = blended.orthonormalized();
        let (position, target) = self.desired_camera();
        self.camera.position = position;
        self.camera.target = target;
        self.camera.up = self.desired_camera_up();
        // Apertura mayor con la velocidad para dar sensacion de impulso.
        let fraction = (self.ship.speed
            / (self.ship.settings.max_speed * self.ship.settings.boost_multiplier))
            .clamp(0.0, 1.0);
        let fov = BASE_FOV + (BOOST_FOV - BASE_FOV) * fraction;
        let fov = self.camera.fov_y + (fov - self.camera.fov_y) * smoothing(4.0, dt);
        let c = self.camera;
        // Los valores del lente ya son validos; si algun dia no lo fueran se conserva el anterior.
        let _ = self.camera.set_lens(fov, c.aspect, c.near, c.far);
    }

    /// Gasto extra de O2 con el turbo: con la nave por encima de su velocidad normal, el
    /// consumo se multiplica por `boost_oxygen_drain`. Solo en partida activa.
    fn drain_boost_oxygen(&mut self, dt: f64) {
        let extra = self.boost_oxygen_drain - 1.0;
        if !(extra.is_finite() && extra > 0.0)
            || self.gameplay.game_over
            || self.gameplay.objective_complete
            || self.ship.speed <= self.ship.settings.max_speed * 1.05
        {
            return;
        }
        let drain = self.gameplay.settings.drain_per_second.max(0.0) * extra * dt;
        self.gameplay.oxygen01 = (self.gameplay.oxygen01 - drain).clamp(0.0, 1.0);
    }

    /// Activa el espacio procedural (planetas errantes y tanques por sectores) y genera los
    /// sectores alrededor de la nave.
    pub fn enable_sectors(&mut self) {
        let mut field = SectorField::new();
        field.update(self.ship.position);
        self.sectors = Some(field);
    }

    /// Paso de los sectores tras el movimiento de la nave desde `previous`: genera los nuevos,
    /// saca a la nave de los planetas errantes y recoge los tanques del trayecto (+O2 como las
    /// capsulas del catalogo, contados en `collected_resources`).
    fn update_sectors(&mut self, previous: Vec3) {
        let Some(field) = self.sectors.as_mut() else {
            return;
        };
        let ship_radius = self.gameplay.navigation.settings.ship_radius;
        let mut position = self.ship.position;
        if field.resolve_collision(previous, &mut position, self.time, ship_radius) {
            self.ship.position = position;
            self.ship.speed = 0.0;
        }
        field.update(self.ship.position);
        if self.gameplay.game_over || self.gameplay.objective_complete {
            return;
        }
        let picked = field.collect_along(
            previous,
            self.ship.position,
            self.gameplay.settings.pickup_radius,
        );
        if picked > 0 {
            self.gameplay.collected_resources += picked;
            let gain = self.gameplay.settings.pickup_recovery * f64::from(picked);
            self.gameplay.oxygen01 = (self.gameplay.oxygen01 + gain).clamp(0.0, 1.0);
        }
    }

    /// Anade la criatura estatica (debug, opt-in). Comprueba antes que la luz del mundo sirve
    /// a su shader, para no descubrirlo al dibujar.
    pub fn set_creature(&mut self, creature: StaticCreature) -> Result<(), String> {
        TexturedShader::new(
            creature.texture(),
            TexturedMode::Lit,
            self.system.light_position(),
            AMBIENT,
        )?;
        // Con el modo por etapas, la nueva particion (y con la mirada, su calibracion) se
        // calcula antes del cambio: si falla, se devuelve el error y la criatura y las caches
        // anteriores siguen intactas.
        let parts = self
            .staged_reveal
            .as_ref()
            .map(|m| StagedParts::split(creature.model(), &m.region))
            .transpose()?;
        let rig = self
            .staged_reveal
            .as_ref()
            .filter(|m| m.eye.is_some())
            .map(|m| EyeRig::new(&creature, &m.region))
            .transpose()?;
        if let Some(mode) = &mut self.staged_reveal {
            mode.parts = parts;
            if let Some(eye) = &mut mode.eye {
                eye.rig = rig;
            }
        }
        self.creature = Some(creature);
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
        Ok(())
    }
    /// Quita la criatura; con el modo por etapas suelta tambien su particion en cache (y con
    /// la mirada, su calibracion y su pose; el estado angular se conserva).
    pub fn clear_creature(&mut self) -> Option<StaticCreature> {
        if let Some(mode) = &mut self.staged_reveal {
            mode.parts = None;
            if let Some(eye) = &mut mode.eye {
                eye.rig = None;
            }
        }
        self.creature.take()
    }
    pub fn creature(&self) -> Option<&StaticCreature> {
        self.creature.as_ref()
    }
    /// Activa la revelacion por oxigeno (debug, opt-in) ya en el objetivo del oxigeno actual.
    /// Sobrevive a `clear_creature`/`set_creature`; sin criatura no dibuja nada. Desactiva el
    /// modo por etapas si estaba activo (excluyentes; gana el ultimo).
    pub fn enable_creature_reveal(&mut self, settings: RevealSettings) {
        self.creature_reveal = Some(CreatureReveal::new(settings, self.debug_oxygen01));
        self.staged_reveal = None;
    }
    /// Vuelve a la criatura estatica siempre visible.
    pub fn disable_creature_reveal(&mut self) {
        self.creature_reveal = None;
    }
    pub fn creature_reveal(&self) -> Option<CreatureReveal> {
        self.creature_reveal
    }
    /// Paso visual del efecto con segundos **reales** transcurridos, separado de `update`: no
    /// usa el `dt` limitado de la fisica, no suma al tiempo orbital y `F5` no lo congela.
    pub fn update_creature_reveal(&mut self, elapsed_seconds: f64) {
        if let Some(reveal) = &mut self.creature_reveal {
            reveal.update(self.debug_oxygen01, elapsed_seconds);
        }
        if let Some(mode) = &mut self.staged_reveal {
            mode.reveal.update(self.debug_oxygen01, elapsed_seconds);
        }
    }
    /// Memoria de los buffers del compositor de revelacion parcial (0 si nunca hizo falta).
    pub fn creature_reveal_buffer_bytes(&self) -> usize {
        self.reveal_compositor.borrow().buffer_bytes()
    }
    /// Activa la revelacion por etapas (debug, opt-in), ya en el objetivo del oxigeno actual.
    /// Si hay criatura, parte su modelo en ojo y cuerpo **ahora** (una vez); si la region no
    /// sirve devuelve `Err` y el mundo queda como estaba. Sustituye a la revelacion uniforme
    /// (V1): los dos modos son excluyentes y gana el ultimo que se activa.
    pub fn enable_staged_creature_reveal(
        &mut self,
        settings: StagedRevealSettings,
        region: EyeRegion,
    ) -> Result<(), String> {
        let parts = self
            .creature
            .as_ref()
            .map(|c| StagedParts::split(c.model(), &region))
            .transpose()?;
        self.staged_reveal = Some(StagedMode {
            reveal: StagedReveal::new(settings, self.debug_oxygen01),
            region,
            parts,
            eye: None,
        });
        self.creature_reveal = None;
        Ok(())
    }
    /// Vuelve a la criatura estatica siempre visible y suelta la particion.
    pub fn disable_staged_creature_reveal(&mut self) {
        self.staged_reveal = None;
    }
    pub fn staged_creature_reveal(&self) -> Option<StagedReveal> {
        self.staged_reveal.as_ref().map(|m| m.reveal)
    }
    /// Particion en cache de la criatura actual (solo con el modo por etapas y criatura).
    pub fn staged_creature_parts(&self) -> Option<&StagedParts> {
        self.staged_reveal.as_ref().and_then(|m| m.parts.as_ref())
    }
    /// Lleva el efecto al objetivo del oxigeno actual sin transicion.
    pub fn snap_creature_reveal(&mut self) {
        if let Some(reveal) = &mut self.creature_reveal {
            reveal.snap(self.debug_oxygen01);
        }
        if let Some(mode) = &mut self.staged_reveal {
            mode.reveal.snap(self.debug_oxygen01);
        }
    }
    /// Activa la mirada local (debug, opt-in), que exige el modo por etapas. Con criatura la
    /// calibra **ahora** con la region de las etapas; si no calibra devuelve `Err` y el mundo
    /// queda como estaba. Arranca ya mirando a la nave (sin transicion).
    pub fn enable_creature_eye_follow(
        &mut self,
        settings: EyeFollowSettings,
    ) -> Result<(), String> {
        self.enable_creature_eye_follow_targeting(settings, EyeTarget::Ship)
    }
    /// Como `enable_creature_eye_follow`, pero mirando a `target` desde el arranque (la
    /// comparacion A/B arranca en la camara): salta a ese objetivo sin transicion.
    pub fn enable_creature_eye_follow_targeting(
        &mut self,
        settings: EyeFollowSettings,
        target: EyeTarget,
    ) -> Result<(), String> {
        let mode = self
            .staged_reveal
            .as_mut()
            .ok_or("la mirada necesita --creature-reveal-stages (modo por etapas activo)")?;
        let rig = self
            .creature
            .as_ref()
            .map(|c| EyeRig::new(c, &mode.region))
            .transpose()?;
        mode.eye = Some(EyeMode {
            follow: EyeFollow::new(settings),
            target,
            initial_target: target,
            rig,
            turn: None,
            horizontal: None,
        });
        self.snap_creature_eye_follow();
        Ok(())
    }
    /// Apaga la mirada y suelta su calibracion y su pose; las etapas y el O2 siguen igual.
    /// Tambien la apagan `enable_creature_reveal`, `disable_staged_creature_reveal` y volver a
    /// llamar a `enable_staged_creature_reveal` (que reinicia el modo por etapas).
    pub fn disable_creature_eye_follow(&mut self) {
        if let Some(mode) = &mut self.staged_reveal {
            mode.eye = None;
        }
    }
    /// Elige a que punto mira el ojo (comparacion A/B; `Ship` por defecto). Solo cambia la
    /// eleccion: no recalibra, no toca la cache ni salta los angulos; `update_creature_eye_follow`
    /// llega al nuevo objetivo con la transicion de siempre. Sin mirada, `Err` y nada cambia.
    pub fn set_creature_eye_target(&mut self, target: EyeTarget) -> Result<(), String> {
        let eye = self
            .staged_reveal
            .as_mut()
            .and_then(|m| m.eye.as_mut())
            .ok_or("el objetivo de la mirada necesita --creature-eye-follow (mirada activa)")?;
        eye.target = target;
        Ok(())
    }
    /// Punto al que mira el ojo (solo con la mirada activa).
    pub fn creature_eye_target(&self) -> Option<EyeTarget> {
        self.eye_mode().map(|e| e.target)
    }
    /// Objetivo angular actual hacia el punto elegido (QA/HUD; `None` sin mirada o sin
    /// criatura). Es lo que persigue `update_creature_eye_follow`.
    pub fn creature_eye_goal(&self) -> Option<GazeAngles> {
        self.eye_target()
    }
    /// Lo que muestra el panel de la comparacion (QA/HUD; `None` sin mirada): el punto
    /// elegido, los angulos actuales, el objetivo angular y el angulo al eje del objetivo
    /// (`None` los dos sin criatura).
    pub fn creature_eye_comparison_status(&self) -> Option<EyeComparisonStatus> {
        let eye = self.eye_mode()?;
        let aimed = eye.rig.as_ref().zip(self.creature.as_ref());
        // Con giro global, el marco compensado (el giro rigido conserva los angulos al eje).
        let point = self
            .creature
            .as_ref()
            .map_or(self.eye_target_point(eye.target), |c| {
                self.eye_aim_point(eye, c)
            });
        Some(EyeComparisonStatus {
            target: eye.target,
            current: eye.follow.current(),
            goal: aimed.map(|(rig, creature)| rig.skin.aim(creature, point)),
            off_axis_degrees: aimed.and_then(|(rig, creature)| {
                off_axis_degrees(
                    creature
                        .transform()
                        .matrix()
                        .transform_point(rig.skin.pivot()),
                    rig.skin.optical_axis(),
                    creature.placement().yaw(),
                    point,
                )
            }),
        })
    }
    /// Estado angular de la mirada (solo con la mirada activa).
    pub fn creature_eye_follow(&self) -> Option<EyeFollow> {
        self.eye_mode().map(|e| e.follow)
    }
    /// Paso de la mirada con segundos **reales** hacia la posicion actual de la nave, como
    /// `update_creature_reveal`: sin el `dt` limitado de la fisica, sin tocar el tiempo orbital
    /// y sin que `F5` la congele. Avanza tambien con la criatura oculta (solo angulos; la
    /// geometria se posa al dibujar). Sin mirada o sin criatura no hace nada.
    ///
    /// Con el giro global (candidata) avanza **antes** el giro con los mismos segundos hacia el
    /// mismo punto, y el ojo persigue despues el objetivo compensado con el giro ya movido.
    pub fn update_creature_eye_follow(&mut self, elapsed_seconds: f64) {
        let goal = self
            .eye_mode()
            .filter(|e| e.turn.is_some())
            .and_then(|eye| {
                let creature = self.creature.as_ref()?;
                let (aim, weight, _) =
                    self.turn_request(creature, self.eye_target_point(eye.target));
                Some(eye.turn?.goal(aim, weight))
            });
        if let (Some(goal), Some(turn)) = (goal, self.distant_turn_mut()) {
            turn.update(goal, elapsed_seconds);
        }
        // Candidata 02: el seguimiento horizontal avanza tambien antes que el ojo, hacia la
        // proyeccion XZ del mismo punto (sin direccion conserva su ultimo objetivo). Sin
        // criatura no avanza.
        let azimuth = self
            .eye_mode()
            .filter(|e| e.horizontal.is_some())
            .and_then(|eye| {
                let placement = self.creature.as_ref()?.placement();
                Some(target_azimuth(
                    placement.center(),
                    placement.yaw(),
                    self.eye_target_point(eye.target),
                ))
            });
        if let (Some(azimuth), Some(follow)) = (azimuth, self.horizontal_follow_mut()) {
            follow.update(azimuth, elapsed_seconds);
        }
        let target = self.eye_target();
        if let (Some(target), Some(eye)) = (
            target,
            self.staged_reveal.as_mut().and_then(|m| m.eye.as_mut()),
        ) {
            eye.follow.update(target, elapsed_seconds);
        }
    }
    /// Poses aplicadas a la geometria en cache desde que se preparo (QA; `None` sin cache).
    pub fn creature_eye_pose_updates(&self) -> Option<usize> {
        Some(self.eye_mode()?.rig.as_ref()?.pose_updates.get())
    }
    /// Angulos en los que esta posada la geometria en cache (QA; `None` sin cache).
    pub fn creature_eye_posed(&self) -> Option<GazeAngles> {
        Some(self.eye_mode()?.rig.as_ref()?.cache.borrow().posed())
    }
    /// Memoria aproximada de la geometria del ojo en cache, en bytes (0 sin cache).
    pub fn creature_eye_cache_bytes(&self) -> usize {
        self.eye_mode()
            .and_then(|e| e.rig.as_ref())
            .map_or(0, |rig| {
                let cache = rig.cache.borrow();
                let m = cache.eye_model();
                m.vertices.len() * size_of::<[f64; 3]>()
                    + m.uvs.len() * size_of::<[f64; 2]>()
                    + m.normals.len() * size_of::<[f64; 3]>()
                    + m.faces.len() * size_of::<Face>()
            })
    }
    /// Calibracion de la criatura actual (QA; solo con la mirada activa y criatura).
    pub fn creature_eye_skin(&self) -> Option<&EyeSkin> {
        self.eye_mode()?.rig.as_ref().map(|r| &r.skin)
    }
    fn eye_mode(&self) -> Option<&EyeMode> {
        self.staged_reveal.as_ref().and_then(|m| m.eye.as_ref())
    }
    /// Angulos hacia el punto elegido desde el pivote del ojo: la posicion de la nave o la de
    /// la camara (nunca el punto al que mira la camara). `None` sin criatura.
    fn eye_target(&self) -> Option<GazeAngles> {
        let eye = self.eye_mode()?;
        let creature = self.creature.as_ref()?;
        let point = self.eye_aim_point(eye, creature);
        Some(eye.rig.as_ref()?.skin.aim(creature, point))
    }
    /// Punto elegido llevado al marco de reposo por la inversa del giro global actual (sin giro
    /// o en reposo, el mismo punto exacto): la calibracion de reposo apunta ahi y el ojo, girado
    /// con la criatura, mira al punto real.
    fn eye_aim_point(&self, eye: &EyeMode, creature: &StaticCreature) -> Vec3 {
        let point = self.eye_target_point(eye.target);
        let placement = creature.placement();
        match (eye.turn, eye.horizontal) {
            (Some(turn), _) => {
                rest_frame_target(placement.center(), placement.yaw(), turn.current(), point)
            }
            (None, Some(follow)) => rest_frame_point(placement.center(), follow.yaw(), point),
            (None, None) => point,
        }
    }
    fn eye_target_point(&self, target: EyeTarget) -> Vec3 {
        match target {
            EyeTarget::Ship => self.ship.position,
            EyeTarget::Camera => self.camera.position,
        }
    }
    fn snap_creature_eye_follow(&mut self) {
        let target = self.eye_target();
        if let (Some(target), Some(eye)) = (
            target,
            self.staged_reveal.as_mut().and_then(|m| m.eye.as_mut()),
        ) {
            eye.follow.snap(target);
        }
    }
    /// Coloca la nave frente a la criatura mirandola (`CreaturePlacement::spawn_pose`). No
    /// cambia el tiempo ni los cuerpos; repetirla da exactamente el mismo arranque.
    pub fn place_near_creature(&mut self) -> Result<(), String> {
        let creature = self
            .creature
            .as_ref()
            .ok_or("No hay criatura: usa --creature-model y --creature-texture")?;
        let (position, forward) = creature.placement().spawn_pose();
        self.ship = Ship::new(position, forward);
        self.snap_camera();
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
        Ok(())
    }

    /// Activa el giro global lejano (candidata, opt-in), que exige la mirada: encendido y en
    /// reposo, sin tocar la mirada ni la cache. `Err` sin mirada, y nada cambia. Lo quitan
    /// `disable_creature_distant_turn` y todo lo que apaga la mirada.
    pub fn enable_creature_distant_turn(&mut self) -> Result<(), String> {
        let eye = self
            .staged_reveal
            .as_mut()
            .and_then(|m| m.eye.as_mut())
            .ok_or("el giro global necesita --creature-eye-compare (mirada activa)")?;
        // Candidata 02: no se apila con el seguimiento horizontal (sin el, como siempre).
        if eye.horizontal.is_some() {
            return Err(
                "el giro global y el seguimiento horizontal (--creature-horizontal-follow) son incompatibles"
                    .into(),
            );
        }
        eye.turn = Some(DistantTurn::new());
        Ok(())
    }
    /// Quita el giro global de golpe (API; las rutas vuelven a ser las de siempre). En el juego
    /// `H` usa `set_creature_distant_turn_active`, que vuelve al reposo con transicion.
    pub fn disable_creature_distant_turn(&mut self) {
        if let Some(eye) = self.staged_reveal.as_mut().and_then(|m| m.eye.as_mut()) {
            eye.turn = None;
        }
    }
    /// `H`: enciende o apaga el giro global; solo cambia la eleccion (apagado, el objetivo es el
    /// reposo y el estado vuelve con su transicion). Sin giro, `Err` sin efectos.
    pub fn set_creature_distant_turn_active(&mut self, active: bool) -> Result<(), String> {
        self.distant_turn_mut()
            .ok_or("el giro global no esta activo (--creature-distant-turn)")?
            .set_active(active);
        Ok(())
    }
    /// Estado del giro global (solo con el giro activo).
    pub fn creature_distant_turn(&self) -> Option<DistantTurn> {
        self.eye_mode()?.turn
    }
    /// Lo que muestra el panel del giro global (solo con el giro activo).
    pub fn creature_distant_turn_status(&self) -> Option<DistantTurnStatus> {
        let eye = self.eye_mode()?;
        let turn = eye.turn?;
        let point = self.eye_target_point(eye.target);
        let request = self.creature.as_ref().map(|c| self.turn_request(c, point));
        Some(DistantTurnStatus {
            turn,
            aim: request.map(|(aim, _, _)| aim),
            goal: request.map(|(aim, weight, _)| turn.goal(aim, weight)),
            distance_radii: request.map(|(_, _, distance)| distance),
            weight: request.map(|(_, weight, _)| weight),
            off_axis: self.creature.as_ref().and_then(|c| {
                let placement = c.placement();
                turn_off_axis(placement.center(), placement.yaw(), point)
            }),
        })
    }
    /// Pedido del giro hacia `point` desde el centro fijo, peso de distancia y distancia de la
    /// camara (el que mira) al centro, en radios.
    fn turn_request(&self, creature: &StaticCreature, point: Vec3) -> (TurnAngles, f64, f64) {
        let placement = creature.placement();
        let distance =
            (self.camera.position - placement.center()).length() / placement.world_radius();
        (
            aim_turn(placement.center(), placement.yaw(), point),
            distance_weight(distance),
            distance,
        )
    }
    /// Activa el seguimiento corporal horizontal (candidata 02, opt-in), que exige la mirada:
    /// encendido, de frente y sin objetivo, sin tocar la mirada ni la cache. Excluyente con el
    /// giro global de la candidata 01 (no se apilan). `Err` sin mirada o con el giro global, y
    /// nada cambia. Lo quitan `disable_creature_horizontal_follow` y todo lo que apaga la mirada.
    pub fn enable_creature_horizontal_follow(&mut self) -> Result<(), String> {
        let eye = self
            .staged_reveal
            .as_mut()
            .and_then(|m| m.eye.as_mut())
            .ok_or("el seguimiento horizontal necesita --creature-eye-compare (mirada activa)")?;
        if eye.turn.is_some() {
            return Err(
                "el seguimiento horizontal y el giro global (--creature-distant-turn) son incompatibles"
                    .into(),
            );
        }
        eye.horizontal = Some(HorizontalFollow::new());
        Ok(())
    }
    /// Quita el seguimiento de golpe (API; las rutas vuelven a ser las de siempre). En el juego
    /// `H` usa `set_creature_horizontal_follow_active`, que vuelve al frente con su dinamica.
    pub fn disable_creature_horizontal_follow(&mut self) {
        if let Some(eye) = self.staged_reveal.as_mut().and_then(|m| m.eye.as_mut()) {
            eye.horizontal = None;
        }
    }
    /// `H`: enciende (persigue el ultimo objetivo) o apaga (vuelve al frente con la misma
    /// dinamica; no es una pausa). Solo cambia la eleccion. Sin seguimiento, `Err` sin efectos.
    pub fn set_creature_horizontal_follow_active(&mut self, active: bool) -> Result<(), String> {
        self.horizontal_follow_mut()
            .ok_or("el seguimiento horizontal no esta activo (--creature-horizontal-follow)")?
            .set_active(active);
        Ok(())
    }
    /// Estado del seguimiento horizontal (solo con el seguimiento activo).
    pub fn creature_horizontal_follow(&self) -> Option<HorizontalFollow> {
        self.eye_mode()?.horizontal
    }
    /// Lo que muestra el panel del seguimiento (solo con el seguimiento activo).
    pub fn creature_horizontal_follow_status(&self) -> Option<HorizontalFollowStatus> {
        let eye = self.eye_mode()?;
        let follow = eye.horizontal?;
        let placement = self.creature.as_ref().map(|c| c.placement());
        Some(HorizontalFollowStatus {
            follow,
            target_azimuth: placement.and_then(|p| {
                target_azimuth(p.center(), p.yaw(), self.eye_target_point(eye.target))
            }),
            has_creature: placement.is_some(),
        })
    }
    fn horizontal_follow_mut(&mut self) -> Option<&mut HorizontalFollow> {
        self.staged_reveal
            .as_mut()?
            .eye
            .as_mut()?
            .horizontal
            .as_mut()
    }
    /// Transform con el que se dibujan el ojo y el cuerpo de la criatura (QA; `None` sin
    /// criatura): el girado si hay giro (01) o seguimiento (02) fuera del reposo, si no el
    /// original exacto.
    pub fn creature_render_transform(&self) -> Option<Transform> {
        let creature = self.creature.as_ref()?;
        Some(
            self.turned_creature_transform(creature)
                .unwrap_or_else(|| creature.transform()),
        )
    }
    fn distant_turn_mut(&mut self) -> Option<&mut DistantTurn> {
        self.staged_reveal.as_mut()?.eye.as_mut()?.turn.as_mut()
    }
    /// Coloca la nave en un preset de la candidata (`CreaturePreset`), mirando al centro.
    /// Lleva el giro global al reposo exacto y despues, con la camara ya colocada, salta la
    /// revelacion y la mirada a su objetivo. Conserva el O2, el objetivo de la mirada y `H`.
    /// Sin criatura, `Err` y nada cambia.
    pub fn place_creature_preset(&mut self, preset: CreaturePreset) -> Result<(), String> {
        let creature = self
            .creature
            .as_ref()
            .ok_or("No hay criatura: usa --creature-model y --creature-texture")?;
        let (position, forward) = preset_pose(&creature.placement(), preset);
        self.ship = Ship::new(position, forward);
        if let Some(turn) = self.distant_turn_mut() {
            turn.reset();
        }
        if let Some(follow) = self.horizontal_follow_mut() {
            follow.reset();
        }
        self.snap_camera();
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
        Ok(())
    }

    /// Activa el Sol con ojo (composicion aprobada 03, opt-in; `render` no lo usa). Exige el
    /// modo por etapas y la criatura **anclada en el centro del Sol** (el cuerpo sin orbita),
    /// que el llamador coloca con su propia `CreaturePlacement` (escala y encuadre son suyos).
    /// Sol y criatura comparten anclaje pero no orientacion: el giro axial de la esfera no
    /// toca la criatura ni su seguimiento. `Err` sin efectos.
    ///
    /// Ciclo de vida: el preview queda **latente** si despues se quita la criatura o se apagan
    /// las etapas (`render_solar_reveal_preview` vuelve a ser `render`) y se reactiva solo
    /// cuando vuelven las dos cosas. Cada pose revalida el Sol en cache; cada cuadro solar
    /// activo revalida el Sol y el anclaje de la criatura. Sin etapas o criatura, o con el ojo
    /// oculto, el cuadro delega a `render` sin exigir validacion solar. Solo lo quita
    /// `disable_solar_reveal_preview`. `SolarRevealLight::Camera` es la luz aprobada con la
    /// composicion del preview 03 (la del juego solar, `visual::solar_game`).
    pub fn enable_solar_reveal_preview(&mut self, light: SolarRevealLight) -> Result<(), String> {
        if self.staged_reveal.is_none() {
            return Err("el preview solar necesita la revelacion por etapas".into());
        }
        let creature = self
            .creature
            .as_ref()
            .ok_or("el preview solar necesita la criatura")?;
        let sun = self
            .system
            .bodies
            .iter()
            .position(|b| is_preview_sun(b, self.time))
            .ok_or("el preview solar necesita un Sol (sol animado sin orbita, radio > 0)")?;
        let center = self.system.bodies[sun].position(self.time);
        if creature.placement().center() != center {
            return Err(format!(
                "la criatura del preview solar debe estar anclada en el centro del Sol {center:?}"
            ));
        }
        self.solar_preview = Some(SolarPreview {
            sun,
            light,
            compositor: RefCell::new(SolarPairCompositor::new()),
        });
        Ok(())
    }
    /// Quita el preview solar; `render_solar_reveal_preview` vuelve a ser `render`.
    pub fn disable_solar_reveal_preview(&mut self) {
        self.solar_preview = None;
    }
    /// Alfas del preview (QA/manifiesto; `None` sin preview o sin etapas), del **progreso
    /// mostrado** de las etapas: avanzan con `update_creature_reveal` y se revierten igual.
    pub fn solar_reveal_alphas(&self) -> Option<SolarRevealAlphas> {
        self.solar_preview.as_ref()?;
        let reveal = self.staged_reveal.as_ref()?.reveal;
        Some(solar_reveal_alphas(reveal.shown(), &reveal.settings()))
    }
    /// Pose de perfil del preview: la nave a `distance` del centro del Sol, en el azimut
    /// `azimuth` (0 = +Z, positivo hacia +X) y a `height` sobre su plano, mirando al centro;
    /// la camara de persecucion queda ~22 unidades detras. Salta la revelacion y la mirada como
    /// los demas arranques (el seguimiento conserva su giro y avanza con segundos reales).
    pub fn place_solar_reveal_camera(
        &mut self,
        azimuth: f64,
        distance: f64,
        height: f64,
    ) -> Result<(), String> {
        let preview = self
            .solar_preview
            .as_ref()
            .ok_or("la pose de perfil necesita el preview solar")?;
        let sun = self.preview_sun(preview)?;
        if !(azimuth.is_finite()
            && distance.is_finite()
            && height.is_finite()
            && distance > sun.body_radius)
        {
            return Err(format!(
                "pose de perfil invalida: azimut {azimuth}, distancia {distance} (> {}), altura {height}",
                sun.body_radius
            ));
        }
        let center = sun.position(self.time);
        let position =
            center + Vec3::new(azimuth.sin(), 0.0, azimuth.cos()) * distance + Vec3::Y * height;
        // Preflight de la pose COMPUESTA antes de tocar nada (la nave con la misma expresion
        // de siempre: los perfiles conocidos no cambian ni un bit).
        let ship = Ship::new(position, (center - position).normalized());
        check_profile_pose(
            &ship,
            center,
            self.chase_distance,
            self.chase_height,
            &self.camera,
        )?;
        self.ship = ship;
        self.snap_camera();
        self.snap_creature_reveal();
        self.snap_creature_eye_follow();
        Ok(())
    }
    /// Sol en cache del preview, revalidado en cada uso: `system` es publico y puede cambiar
    /// despues de activar (cuerpos quitados, reordenados o editados). `Err` si el indice ya no
    /// existe o ese cuerpo ya no cumple `is_preview_sun`.
    fn preview_sun(&self, preview: &SolarPreview) -> Result<&CelestialBody, String> {
        self.system
            .bodies
            .get(preview.sun)
            .filter(|b| is_preview_sun(b, self.time))
            .ok_or_else(|| {
                "el Sol del preview ya no existe o ya no es un Sol (sol animado sin orbita, radio finito > 0)"
                    .into()
            })
    }
    /// Memoria de los buffers del compositor del preview (0 sin preview o sin usar).
    pub fn solar_reveal_buffer_bytes(&self) -> usize {
        self.solar_preview
            .as_ref()
            .map_or(0, |p| p.compositor.borrow().buffer_bytes())
    }
    /// Cuadro del preview solar (no lo usa `render`, que sigue igual). Sin preview, etapas o
    /// criatura, o con el ojo oculto (O2 >= 50 %: Sol entero), es **exactamente** `render`.
    /// Si no: estrellas, planetas y nave como siempre y, lo ultimo del 3D, la pareja central
    /// con `SolarPairCompositor`: la representacion del Sol con `solar_alpha` (con 0 ni se
    /// dibuja; el cuerpo celeste, su orbita, su luz y los planetas no cambian) y el ojo y el
    /// cuerpo de la criatura con sus alfas aprobados, su pose (mirada y seguimiento 02) y la
    /// luz local del preview.
    pub fn render_solar_reveal_preview(&self, fb: &mut Framebuffer) -> Result<RenderStats, String> {
        self.render_solar_reveal_preview_with(fb, None)
    }
    /// `render_solar_reveal_preview` con una capa `Body` alternativa opcional (`None`: exactamente
    /// el mismo cuadro y los mismos contadores). Con `Some`, TODO lo del override (particion
    /// vigente como prefijo exacto, indices, finitud, esfera y shader) se valida ANTES de limpiar
    /// el cuadro: un `Err` deja color y profundidad intactos. Con el ojo oculto (O2 >= 50 %) el
    /// override no se dibuja: la criatura entera sigue invisible como siempre.
    pub fn render_solar_reveal_preview_with(
        &self,
        fb: &mut Framebuffer,
        body: Option<&SolarBodyOverride<'_>>,
    ) -> Result<RenderStats, String> {
        // Some es un contrato explicito incluso si el ojo esta oculto; None conserva el fallback.
        let extra_sphere = if let Some(o) = body {
            let (Some(preview), Some(mode), Some(creature)) =
                (&self.solar_preview, &self.staged_reveal, &self.creature)
            else {
                return Err(
                    "capa Body alternativa requiere preview solar, etapas y criatura".into(),
                );
            };
            let sun = self.preview_sun(preview)?;
            if creature.placement().center() != sun.position(self.time) {
                return Err("capa Body alternativa: criatura no anclada al Sol".into());
            }
            Some(self.validate_body_override(preview, mode, creature, o)?)
        } else {
            None
        };
        let (Some(preview), Some(mode), Some(creature)) =
            (&self.solar_preview, &self.staged_reveal, &self.creature)
        else {
            return Ok(self.render(fb));
        };
        let alphas = solar_reveal_alphas(mode.reveal.shown(), &mode.reveal.settings());
        if alphas.eye <= 0.0 {
            return Ok(self.render(fb));
        }
        let sun = self.preview_sun(preview)?;
        let center = sun.position(self.time);
        if creature.placement().center() != center {
            return Err(format!(
                "la criatura del preview solar ya no esta anclada en el centro del Sol {center:?}"
            ));
        }
        // Some ya paso el preflight, incluso antes del fallback por alfa.
        fb.clear();
        let mut stats = RenderStats::default();
        let context = self.context();
        draw_stars(fb, &self.camera);
        let frustum = self.camera.frustum();
        for (index, body) in self.system.bodies.iter().enumerate() {
            if index != preview.sun {
                stats += self.draw_body(fb, index, body, &context, &frustum);
            }
        }
        stats += self.draw_sector_planets(fb, &context, &frustum);
        let ship_transform = self.ship.transform(SHIP_MODEL_YAW);
        stats.objects_submitted += 1;
        stats += RenderStats {
            objects_submitted: 0,
            ..draw_model(
                fb,
                &self.ship_model,
                &ship_transform,
                &Material {
                    cull_backfaces: self.toggles.backface_culling,
                    ..Material::from_model()
                },
                ShaderKind::LitPlanet,
                &context,
                &self.camera,
            )
        };
        self.draw_collectibles(fb, &context, &mut stats);
        // Criatura: la pose y el descarte de siempre (el ojo posado se posa solo si cambian
        // los angulos), con el mismo transform para ojo y cuerpo.
        let parts = mode
            .parts
            .as_ref()
            .ok_or("modo por etapas sin particion de la criatura")?;
        let transform = self
            .turned_creature_transform(creature)
            .unwrap_or_else(|| creature.transform());
        let posed = self.posed_eye();
        let cull_radius = posed.map_or(creature.placement().world_radius(), |(_, rig, _)| {
            rig.cull_radius
        });
        let creature_culled = self.toggles.frustum_culling
            && !frustum.sphere_visible(center, cull_radius)
            && extra_sphere.is_none_or(|(c, r)| !frustum.sphere_visible(c, r));
        let cache = match posed {
            Some((_, rig, angles)) if !creature_culled => {
                let mut cache = rig.cache.borrow_mut();
                if cache.posed() != angles {
                    cache.pose(creature.model(), &rig.skin, angles)?;
                    rig.pose_updates.set(rig.pose_updates.get() + 1);
                }
                drop(cache);
                Some(rig.cache.borrow())
            }
            _ => None,
        };
        let eye_model = cache.as_ref().map_or(parts.eye(), |c| c.eye_model());
        let light = match preview.light {
            SolarRevealLight::World => context.light_position,
            SolarRevealLight::Camera => self.camera.position,
        };
        let shader = TexturedShader::new(creature.texture(), TexturedMode::Lit, light, AMBIENT)?;
        let creature_shade = |f: &FragmentInput, _: &Material| shader.shade(f);
        let sun_transform = sun.transform(self.time);
        let sun_shade =
            |f: &FragmentInput, m: &Material| shade_fragment(sun.shader, f, &context, m);
        let mut layers = Vec::with_capacity(3);
        if alphas.sun > 0.0 {
            stats.objects_submitted += 1;
            let level = self.lod_level(
                preview.sun,
                sun.body_radius,
                (sun_transform.position - self.camera.position).length(),
            );
            let sphere = &self.sphere_lods[level];
            let radius = sphere.bounds_radius * sun.body_radius;
            if self.toggles.frustum_culling
                && !frustum.sphere_visible(sun_transform.position, radius)
            {
                stats.objects_culled += 1;
                stats.triangles_submitted += sphere.faces.len();
                stats.triangles_culled += sphere.faces.len();
            } else {
                layers.push(PairLayer {
                    kind: PairLayerKind::Sun,
                    model: sphere,
                    transform: &sun_transform,
                    material: Material {
                        cull_backfaces: self.toggles.backface_culling,
                        ..sun.material
                    },
                    alpha: alphas.sun,
                    shade: &sun_shade,
                });
            }
        }
        stats.objects_submitted += 1;
        if creature_culled {
            let faces = body.map_or(creature.model().faces.len(), |o| {
                o.model.faces.len() + parts.eye().faces.len()
            });
            stats.objects_culled += 1;
            stats.triangles_submitted += faces;
            stats.triangles_culled += faces;
        } else {
            for (kind, model, alpha) in [
                (
                    PairLayerKind::Body,
                    body.map_or(parts.body(), |o| o.model),
                    alphas.body,
                ),
                (PairLayerKind::Eye, eye_model, alphas.eye),
            ] {
                layers.push(PairLayer {
                    kind,
                    model,
                    transform: &transform,
                    material: Material::from_model(),
                    alpha,
                    shade: &creature_shade,
                });
            }
        }
        let mut drawn = preview
            .compositor
            .borrow_mut()
            .render(fb, &self.camera, &layers)?;
        drawn.objects_submitted = 0; // ya contados arriba
        stats += drawn;
        Ok(stats)
    }
    /// Readonly explicit-Body preflight for observers that advance without drawing. This is the
    /// existing render preflight, including solar source checks; no alpha fallback for an override.
    pub(crate) fn validate_solar_body_override(
        &self,
        body: &SolarBodyOverride<'_>,
    ) -> Result<(), String> {
        let (Some(preview), Some(mode), Some(creature)) =
            (&self.solar_preview, &self.staged_reveal, &self.creature)
        else {
            return Err("capa Body alternativa requiere preview solar, etapas y criatura".into());
        };
        let sun = self.preview_sun(preview)?;
        if creature.placement().center() != sun.position(self.time) {
            return Err("capa Body alternativa: criatura no anclada al Sol".into());
        }
        self.validate_body_override(preview, mode, creature, body)?;
        Ok(())
    }

    /// Preflight de la capa `Body` alternativa (sin tocar nada): particion vigente como PREFIJO exacto
    /// de vertices, UV y normales; caras con UV y normal e indices en rango; todo finito; esfera del
    /// apendice finita; shader de la criatura valido con la luz de este cuadro. Devuelve la esfera del
    /// apendice EN MUNDO con el transform de dibujo vigente (el girado si hay seguimiento).
    fn validate_body_override(
        &self,
        preview: &SolarPreview,
        mode: &StagedMode,
        creature: &StaticCreature,
        o: &SolarBodyOverride<'_>,
    ) -> Result<(Vec3, f64), String> {
        let parts = mode
            .parts
            .as_ref()
            .ok_or("modo por etapas sin particion de la criatura")?;
        let (b, m) = (parts.body(), o.model);
        let same_prefix = m.vertices.len() > b.vertices.len()
            && m.vertices[..b.vertices.len()] == b.vertices[..]
            && m.uvs.len() >= b.uvs.len()
            && m.uvs[..b.uvs.len()] == b.uvs[..]
            && m.normals.len() >= b.normals.len()
            && m.normals[..b.normals.len()] == b.normals[..];
        if !same_prefix {
            return Err(
                "capa Body alternativa: no parte de la particion vigente de la criatura (prefijo de vertices/UV/normales distinto)"
                    .into(),
            );
        }
        let finite = |v: &[f64]| v.iter().all(|c| c.is_finite());
        if !(m.vertices[b.vertices.len()..].iter().all(|v| finite(v))
            && m.uvs[b.uvs.len()..].iter().all(|v| finite(v))
            && m.normals[b.normals.len()..].iter().all(|v| finite(v)))
        {
            return Err("capa Body alternativa: valores no finitos".into());
        }
        for f in &m.faces {
            let (Some(uv), Some(n)) = (f.uv_indices, f.normal_indices) else {
                return Err("capa Body alternativa: cara sin UV o sin normal por esquina".into());
            };
            if f.vertex_indices.iter().any(|&i| i >= m.vertices.len())
                || uv.iter().any(|&i| i >= m.uvs.len())
                || n.iter().any(|&i| i >= m.normals.len())
            {
                return Err("capa Body alternativa: indice fuera de rango".into());
            }
        }
        let (c, r) = o.extra_bounds_obj;
        if !(c.is_finite() && r.is_finite() && r >= 0.0) {
            return Err("capa Body alternativa: esfera del apendice invalida".into());
        }
        for v in &m.vertices[b.vertices.len()..] {
            let distance = (Vec3::from_array(*v) - c).length();
            if !(distance.is_finite() && distance <= r) {
                return Err("capa Body alternativa: esfera no contiene el apendice".into());
            }
        }
        let transform = self
            .turned_creature_transform(creature)
            .unwrap_or_else(|| creature.transform());
        let matrix = transform.matrix();
        let center = matrix.transform_point(c);
        let radius = r * transform.scale.x.abs();
        if !(center.is_finite()
            && radius.is_finite()
            && matrix.m.iter().flatten().all(|v| v.is_finite()))
        {
            return Err("capa Body alternativa: transform o esfera en mundo no finitos".into());
        }
        let mvp = self.camera.view_projection() * matrix;
        if !mvp.m.iter().flatten().all(|v| v.is_finite()) {
            return Err("capa Body alternativa: matriz compuesta MVP no finita".into());
        }
        // La etapa de vertices usa TODAS las posiciones/normales, no solo la esfera de descarte.
        for v in &m.vertices {
            let v = Vec3::from_array(*v);
            let clip = mvp.transform(v.extend(1.0));
            if !(matrix.transform_point(v).is_finite()
                && [clip.x, clip.y, clip.z, clip.w]
                    .iter()
                    .all(|v| v.is_finite()))
            {
                return Err("capa Body alternativa: posicion transformada no finita".into());
            }
        }
        if !(m.uvs.iter().flatten().all(|v| v.is_finite())
            && m.normals
                .iter()
                .all(|n| transform.transform_normal(Vec3::from_array(*n)).is_finite()))
        {
            return Err("capa Body alternativa: atributo transformado no finito".into());
        }
        let light = match preview.light {
            SolarRevealLight::World => self.system.light_position(),
            SolarRevealLight::Camera => self.camera.position,
        };
        TexturedShader::new(creature.texture(), TexturedMode::Lit, light, AMBIENT)?;
        Ok((center, radius))
    }

    /// Props procedurales del cuerpo `index`: cada uno con `SurfaceAnchor::world_transform` sobre
    /// la `Transform` actual del cuerpo y su luz sombreada por el planeta (`prop_light_position`).
    /// Forman parte del cuerpo logico: no suman `objects_submitted` ni `objects_culled`, solo sus
    /// triangulos y fragmentos. Se omiten en el nivel de detalle mas grueso (planeta lejano, props
    /// de pocos pixeles) y con el interruptor F2 apagado.
    fn draw_planet_props(
        &self,
        fb: &mut Framebuffer,
        body_name: &str,
        transform: &Transform,
        level: usize,
        context: &ShaderContext,
        frustum: &Frustum,
    ) -> RenderStats {
        let Some(content) = self
            .planet_props
            .iter()
            .flatten()
            .find(|content| content.body == body_name)
        else {
            return RenderStats::default();
        };
        self.draw_props_content(fb, content, transform, level, context, frustum)
    }

    /// Dibuja un conjunto de props sobre el cuerpo con `transform` (sistema o sectores).
    pub(crate) fn draw_props_content(
        &self,
        fb: &mut Framebuffer,
        content: &PlanetProps,
        transform: &Transform,
        level: usize,
        context: &ShaderContext,
        frustum: &Frustum,
    ) -> RenderStats {
        let mut stats = RenderStats::default();
        if !self.toggles.planet_props || level + 1 >= LOD_RESOLUTIONS.len() {
            return stats;
        }
        let radius = transform.scale.x.abs();
        let culled = |stats: &mut RenderStats, faces: usize| {
            stats.triangles_submitted += faces;
            stats.triangles_culled += faces;
        };
        // Descarte del conjunto: la esfera del planeta mas la altura maxima de sus props.
        if self.toggles.frustum_culling
            && !frustum.sphere_visible(transform.position, radius * (1.0 + content.max_reach))
        {
            for prop in &content.props {
                culled(&mut stats, content.meshes[prop.mesh].faces.len());
            }
            return stats;
        }
        let planet = transform.matrix();
        let material = Material {
            ambient: 0.06,
            cull_backfaces: self.toggles.backface_culling,
            ..Material::from_model()
        };
        for prop in &content.props {
            let mesh = &content.meshes[prop.mesh];
            // Validado en `World::new` con el mismo tipo de transform; un `Err` aqui solo
            // podria venir de un planeta degenerado y entonces el prop no se dibuja.
            let Ok(pose) = prop.anchor.world_transform(transform) else {
                continue;
            };
            let scale = pose.scale.x.abs();
            let center = pose
                .matrix()
                .transform_point(Vec3::from_array(mesh.bounds_center));
            if self.toggles.frustum_culling
                && !frustum.sphere_visible(center, mesh.bounds_radius * scale)
            {
                culled(&mut stats, mesh.faces.len());
                continue;
            }
            let normal = planet.transform_dir(prop.normal).normalized();
            let lit = ShaderContext {
                light_position: prop_light_position(
                    context.light_position,
                    transform.position,
                    normal,
                    pose.position,
                ),
                ..*context
            };
            stats += RenderStats {
                objects_submitted: 0,
                ..draw_model(
                    fb,
                    mesh,
                    &pose,
                    &material,
                    ShaderKind::LitPlanet,
                    &lit,
                    &self.camera,
                )
            };
        }
        stats
    }

    /// Un cuerpo celeste exactamente como en `render` (LOD con histeresis, descarte y contadores).
    fn draw_body(
        &self,
        fb: &mut Framebuffer,
        index: usize,
        body: &CelestialBody,
        context: &ShaderContext,
        frustum: &Frustum,
    ) -> RenderStats {
        let mut stats = RenderStats::default();
        let transform = body.transform(self.time);
        stats.objects_submitted += 1;
        let level = self.lod_level(
            index,
            body.body_radius,
            (transform.position - self.camera.position).length(),
        );
        stats += self.draw_planet_props(fb, body.name, &transform, level, context, frustum);
        let sphere = &self.sphere_lods[level];
        let radius = sphere.bounds_radius * body.body_radius;
        if self.toggles.frustum_culling && !frustum.sphere_visible(transform.position, radius) {
            stats.objects_culled += 1;
            stats.triangles_submitted += sphere.faces.len();
            stats.triangles_culled += sphere.faces.len();
            return stats;
        }
        let mut drawn = draw_model(
            fb,
            sphere,
            &transform,
            &Material {
                cull_backfaces: self.toggles.backface_culling,
                ..body.material
            },
            body.shader,
            context,
            &self.camera,
        );
        drawn.objects_submitted = 0; // ya contado arriba
        stats += drawn;
        stats
    }

    fn context(&self) -> ShaderContext {
        ShaderContext {
            elapsed_time: self.time,
            oxygen01: self.oxygen01(),
            camera_position: self.camera.position,
            ship_position: self.ship.position,
            light_position: self.system.light_position(),
        }
    }

    /// Descarte conservador sobre el centro real del modelo, antes de transformar vertices.
    fn draw_instance(
        &self,
        fb: &mut Framebuffer,
        model: &Model,
        transform: &Transform,
        material: &Material,
        shader: ShaderKind,
        context: &ShaderContext,
    ) -> RenderStats {
        let center = transform
            .matrix()
            .transform_point(Vec3::from_array(model.bounds_center));
        let radius = model.bounds_radius
            * transform
                .scale
                .x
                .abs()
                .max(transform.scale.y.abs())
                .max(transform.scale.z.abs());
        if self.toggles.frustum_culling && !self.camera.frustum().sphere_visible(center, radius) {
            return RenderStats {
                objects_submitted: 1,
                objects_culled: 1,
                triangles_submitted: model.faces.len(),
                triangles_culled: model.faces.len(),
                ..RenderStats::default()
            };
        }
        draw_model(
            fb,
            model,
            transform,
            material,
            shader,
            context,
            &self.camera,
        )
    }

    fn draw_collectibles(
        &self,
        fb: &mut Framebuffer,
        context: &ShaderContext,
        stats: &mut RenderStats,
    ) {
        if self.sectors.is_some() {
            self.draw_tanks_and_logs(fb, context, stats);
            return;
        }
        let model = &self.sphere_lods[2];
        for (anchor, is_log) in self
            .gameplay
            .resources
            .iter()
            .map(|r| (r, false))
            .chain(self.gameplay.logs.iter().map(|log| (&log.anchor, true)))
        {
            if anchor.collected {
                continue;
            }
            if let Some(position) = anchor.position(&self.system, self.time) {
                let transform = Transform {
                    position,
                    rotation: Vec3::new(0.0, self.time * 1.5, 0.0),
                    scale: Vec3::splat(if is_log { 1.8 } else { 2.4 }),
                };
                let material = if is_log {
                    Material::solid(0.95, 0.55, 0.12)
                } else {
                    Material::solid(0.12, 0.9, 0.75)
                };
                *stats +=
                    self.draw_instance(fb, model, &transform, &material, ShaderKind::Flat, context);
            }
        }
    }

    /// Con sectores: tanques de oxigeno modelados (catalogo y sectores), con pulso de escala y
    /// sin sombreado para que se distingan de lejos, y registros dorados mas grandes.
    fn draw_tanks_and_logs(
        &self,
        fb: &mut Framebuffer,
        context: &ShaderContext,
        stats: &mut RenderStats,
    ) {
        let pulse = 1.0 + 0.12 * (self.time * 4.0).sin();
        let tank_material = Material::from_model();
        let tank = |position: Vec3, phase: f64| Transform {
            position,
            rotation: Vec3::new(0.35, self.time * 1.2 + phase, 0.0),
            scale: Vec3::splat(3.4 * pulse),
        };
        for resource in self.gameplay.resources.iter().filter(|r| !r.collected) {
            if let Some(position) = resource.position(&self.system, self.time) {
                *stats += self.draw_instance(
                    fb,
                    &self.tank_model,
                    &tank(position, resource.id as f64),
                    &tank_material,
                    ShaderKind::Flat,
                    context,
                );
            }
        }
        if let Some(field) = &self.sectors {
            for t in field.tanks() {
                if (t.position - self.camera.position).length() > SECTOR_DRAW_DISTANCE {
                    continue;
                }
                *stats += self.draw_instance(
                    fb,
                    &self.tank_model,
                    &tank(t.position, (t.id % 97) as f64),
                    &tank_material,
                    ShaderKind::Flat,
                    context,
                );
            }
        }
        for log in self.gameplay.logs.iter().filter(|l| !l.anchor.collected) {
            if let Some(position) = log.anchor.position(&self.system, self.time) {
                let transform = Transform {
                    position,
                    rotation: Vec3::new(0.0, self.time * 1.5, 0.0),
                    scale: Vec3::splat(2.8 * pulse),
                };
                *stats += self.draw_instance(
                    fb,
                    &self.sphere_lods[1],
                    &transform,
                    &Material::solid(1.0, 0.68, 0.15),
                    ShaderKind::Flat,
                    context,
                );
            }
        }
    }

    /// Planetas errantes de los sectores: esfera con LOD por tamano aparente (sin histeresis),
    /// descarte por frustum y sus props. Cuentan como objetos.
    fn draw_sector_planets(
        &self,
        fb: &mut Framebuffer,
        context: &ShaderContext,
        frustum: &Frustum,
    ) -> RenderStats {
        let mut stats = RenderStats::default();
        let Some(field) = &self.sectors else {
            return stats;
        };
        for planet in field.planets() {
            let distance = (planet.center - self.camera.position).length();
            if distance > SECTOR_DRAW_DISTANCE + planet.radius {
                continue;
            }
            let transform = planet.transform(self.time);
            let ratio = planet.radius / distance.max(1e-9);
            let level = if !self.toggles.lod {
                0
            } else {
                LOD_MIN_RATIO
                    .iter()
                    .position(|&min| ratio >= min)
                    .unwrap_or(LOD_MIN_RATIO.len())
            };
            if let Some(props) = &planet.props {
                stats += self.draw_props_content(fb, props, &transform, level, context, frustum);
            }
            let sphere = &self.sphere_lods[level];
            stats.objects_submitted += 1;
            if self.toggles.frustum_culling
                && !frustum.sphere_visible(planet.center, sphere.bounds_radius * planet.radius)
            {
                stats.objects_culled += 1;
                stats.triangles_submitted += sphere.faces.len();
                stats.triangles_culled += sphere.faces.len();
                continue;
            }
            let [r, g, b] = planet.color;
            stats += RenderStats {
                objects_submitted: 0,
                ..draw_model(
                    fb,
                    sphere,
                    &transform,
                    &Material {
                        cull_backfaces: self.toggles.backface_culling,
                        ..Material::solid(r, g, b)
                    },
                    ShaderKind::LitPlanet,
                    context,
                    &self.camera,
                )
            };
        }
        stats
    }

    /// Dibuja el cuadro completo: fondo, estrellas, cuerpos, balizas y nave.
    pub fn render(&self, fb: &mut Framebuffer) -> RenderStats {
        fb.clear();
        let mut stats = RenderStats::default();
        let context = self.context();
        draw_stars(fb, &self.camera);
        let frustum = self.camera.frustum();
        for (index, body) in self.system.bodies.iter().enumerate() {
            let transform = body.transform(self.time);
            stats.objects_submitted += 1;
            let level = self.lod_level(
                index,
                body.body_radius,
                (transform.position - self.camera.position).length(),
            );
            stats += self.draw_planet_props(fb, body.name, &transform, level, &context, &frustum);
            let sphere = &self.sphere_lods[level];
            let radius = sphere.bounds_radius * body.body_radius;
            if self.toggles.frustum_culling && !frustum.sphere_visible(transform.position, radius) {
                stats.objects_culled += 1;
                stats.triangles_submitted += sphere.faces.len();
                stats.triangles_culled += sphere.faces.len();
                continue;
            }
            let mut drawn = draw_model(
                fb,
                sphere,
                &transform,
                &Material {
                    cull_backfaces: self.toggles.backface_culling,
                    ..body.material
                },
                body.shader,
                &context,
                &self.camera,
            );
            drawn.objects_submitted = 0; // ya contado arriba
            stats += drawn;
        }
        stats += self.draw_sector_planets(fb, &context, &frustum);
        // Sin revelacion la criatura es opaca (1). En 0 ni se dibuja ni se cuenta, igual que
        // sin criatura; en 1 se dibuja aqui, como la estatica; parcial, al final (ver abajo).
        // Con etapas, `alpha` es la del ojo: el cuerpo solo es positivo con el ojo en 1, y la
        // ruta estatica exige ambos en 1.
        let alpha = self.creature_reveal.map_or(1.0, |r| r.shown());
        let (alpha, body_alpha) = self
            .staged_reveal
            .as_ref()
            .map_or((alpha, alpha), |m| m.reveal.alphas());
        let creature = self.creature.as_ref().filter(|_| alpha > 0.0);
        let creature_frustum = self.toggles.frustum_culling.then_some(&frustum);
        let opaque = alpha >= 1.0 && body_alpha >= 1.0;
        // Giro global (candidata) fuera del reposo: ojo y cuerpo con el mismo transform girado,
        // aunque el ojo este en reposo. En reposo, `None` y las rutas de siempre exactas.
        let turned = creature.and_then(|c| self.turned_creature_transform(c));
        if let Some(creature) = creature.filter(|_| opaque) {
            let transform = turned.unwrap_or_else(|| creature.transform());
            let drawn = match (self.posed_eye(), turned) {
                (None, None) => {
                    creature.render(fb, &self.camera, creature_frustum, context.light_position)
                }
                (None, Some(_)) => self.render_staged(
                    fb,
                    creature,
                    self.staged_creature_parts(),
                    &transform,
                    creature_frustum,
                    context.light_position,
                    (1.0, 1.0),
                ),
                (Some((parts, rig, angles)), _) => self.render_posed_eye(
                    fb,
                    creature,
                    (parts, rig, angles),
                    &transform,
                    creature_frustum,
                    context.light_position,
                    (1.0, 1.0),
                ),
            };
            match drawn {
                Ok(drawn) => stats += drawn,
                // `set_creature` ya valido la luz y `light_position` es constante.
                Err(error) => debug_assert!(false, "criatura sin dibujar: {error}"),
            }
        }
        self.draw_collectibles(fb, &context, &mut stats);
        let ship_transform = self.ship.transform(SHIP_MODEL_YAW);
        stats.objects_submitted += 1;
        stats += RenderStats {
            objects_submitted: 0,
            ..draw_model(
                fb,
                &self.ship_model,
                &ship_transform,
                &Material {
                    cull_backfaces: self.toggles.backface_culling,
                    ..Material::from_model()
                },
                ShaderKind::LitPlanet,
                &context,
                &self.camera,
            )
        };
        // Parcial: fantasma de una capa sobre el cuadro real completo (nave incluida). Tiene
        // que ser lo ultimo en 3D: deja su profundidad en el z-buffer (`RevealCompositor`).
        if let Some(creature) = creature.filter(|_| !opaque) {
            let transform = turned.unwrap_or_else(|| creature.transform());
            let drawn = match &self.staged_reveal {
                None => self.reveal_compositor.borrow_mut().render(
                    fb,
                    creature,
                    &self.camera,
                    creature_frustum,
                    context.light_position,
                    alpha,
                ),
                Some(mode) => match self.posed_eye() {
                    None => self.render_staged(
                        fb,
                        creature,
                        mode.parts.as_ref(),
                        &transform,
                        creature_frustum,
                        context.light_position,
                        (alpha, body_alpha),
                    ),
                    Some(posed) => self.render_posed_eye(
                        fb,
                        creature,
                        posed,
                        &transform,
                        creature_frustum,
                        context.light_position,
                        (alpha, body_alpha),
                    ),
                },
            };
            match drawn {
                Ok(drawn) => stats += drawn,
                Err(error) => debug_assert!(false, "criatura sin dibujar: {error}"),
            }
        }
        stats
    }

    /// Etapas parciales, al final del 3D: el descarte usa la esfera de la criatura completa
    /// (descartada se cuenta como la estatica y no se copia nada); despues el ojo y, solo con
    /// el ojo opaco, el cuerpo, cada uno con `RevealCompositor::render_part` y la textura y
    /// el transform de la criatura (`transform`: el original o, con giro global, el girado
    /// alrededor del mismo centro). Cuenta **una** criatura logica y los vertices, triangulos y
    /// fragmentos que dibujan sus dos partes.
    #[allow(clippy::too_many_arguments)]
    fn render_staged(
        &self,
        fb: &mut Framebuffer,
        creature: &StaticCreature,
        parts: Option<&StagedParts>,
        transform: &Transform,
        frustum: Option<&Frustum>,
        light_position: Vec3,
        (eye_alpha, body_alpha): (f64, f64),
    ) -> Result<RenderStats, String> {
        let placement = creature.placement();
        if frustum.is_some_and(|f| !f.sphere_visible(placement.center(), placement.world_radius()))
        {
            return creature.render(fb, &self.camera, frustum, light_position);
        }
        let parts = parts.ok_or("modo por etapas sin particion de la criatura")?;
        self.draw_parts(
            fb,
            creature,
            (parts.eye(), parts.body()),
            transform,
            light_position,
            (eye_alpha, body_alpha),
        )
    }

    /// Mirada activa con criatura que hay que dibujar posada: la particion, la calibracion y los
    /// angulos. `None` (rutas de siempre, exactas) sin mirada o en reposo exacto con una
    /// envolvente posada que cabe en la esfera original; si no cabe, nunca se finge la esfera.
    fn posed_eye(&self) -> Option<(Option<&StagedParts>, &EyeRig, GazeAngles)> {
        let mode = self.staged_reveal.as_ref()?;
        let eye = mode.eye.as_ref()?;
        let rig = eye.rig.as_ref()?;
        let angles = eye.follow.current();
        if angles.is_neutral() && rig.skin.fits_bind_sphere() {
            return None;
        }
        Some((mode.parts.as_ref(), rig, angles))
    }

    /// Transform girado de la criatura con el giro global fuera del reposo (`None` sin giro o
    /// en reposo exacto: rutas de siempre). Rigido alrededor de `placement.center()`, asi que
    /// la esfera de descarte de siempre (la completa o la del ojo posado) sigue valiendo.
    fn turned_creature_transform(&self, creature: &StaticCreature) -> Option<Transform> {
        let eye = self.eye_mode()?;
        if let Some(follow) = eye.horizontal {
            // Candidata 02: giro rigido alrededor de +Y por el centro (0: rutas de siempre).
            let yaw = follow.yaw();
            return (yaw != 0.0).then(|| horizontal_transform(creature, yaw));
        }
        let turn = eye.turn?.current();
        (!turn.is_neutral()).then(|| turned_transform(creature, turn))
    }

    /// Ojo posado (en cache, se posa solo si cambian los angulos) y cuerpo de reposo de la
    /// particion de las etapas, con la textura y el transform de la criatura original. El
    /// descarte usa la envolvente posada **antes** de posar o transformar nada; descartada se
    /// cuenta como la estatica (caras completas enviadas y descartadas, 0 vertices).
    #[allow(clippy::too_many_arguments)]
    fn render_posed_eye(
        &self,
        fb: &mut Framebuffer,
        creature: &StaticCreature,
        (parts, rig, angles): (Option<&StagedParts>, &EyeRig, GazeAngles),
        transform: &Transform,
        frustum: Option<&Frustum>,
        light_position: Vec3,
        alphas: (f64, f64),
    ) -> Result<RenderStats, String> {
        let center = creature.placement().center();
        if frustum.is_some_and(|f| !f.sphere_visible(center, rig.cull_radius)) {
            let faces = creature.model().faces.len();
            return Ok(RenderStats {
                objects_submitted: 1,
                objects_culled: 1,
                triangles_submitted: faces,
                triangles_culled: faces,
                ..RenderStats::default()
            });
        }
        let parts = parts.ok_or("modo por etapas sin particion de la criatura")?;
        let mut cache = rig.cache.borrow_mut();
        if cache.posed() != angles {
            cache.pose(creature.model(), &rig.skin, angles)?;
            rig.pose_updates.set(rig.pose_updates.get() + 1);
        }
        self.draw_parts(
            fb,
            creature,
            (cache.eye_model(), parts.body()),
            transform,
            light_position,
            alphas,
        )
    }

    /// El ojo y, solo con el cuerpo positivo, el cuerpo, cada uno con
    /// `RevealCompositor::render_part` y el **mismo** `transform`. Cuenta **una** criatura logica.
    fn draw_parts(
        &self,
        fb: &mut Framebuffer,
        creature: &StaticCreature,
        (eye, body): (&Model, &Model),
        transform: &Transform,
        light_position: Vec3,
        (eye_alpha, body_alpha): (f64, f64),
    ) -> Result<RenderStats, String> {
        debug_assert!(
            body_alpha <= 0.0 || eye_alpha >= 1.0,
            "cuerpo con ojo parcial"
        );
        let part = |model| RevealPart {
            model,
            transform,
            texture: creature.texture(),
        };
        let mut compositor = self.reveal_compositor.borrow_mut();
        let mut stats =
            compositor.render_part(fb, part(eye), &self.camera, light_position, eye_alpha)?;
        if body_alpha > 0.0 {
            stats +=
                compositor.render_part(fb, part(body), &self.camera, light_position, body_alpha)?;
        }
        stats.objects_submitted = 1;
        Ok(stats)
    }
}

/// Comprueba, sin mutar nada, que la pose de perfil `ship` y la camara de persecucion que
/// le pondria `snap_camera` (misma formula que `desired_camera`) son usables: nave dentro de
/// `MAX_WORLD_COORD` (el contrato de coordenadas de la criatura), mirando al centro con una
/// base ortonormal finita (direccion nula por desborde o coincidencia da base degenerada), y
/// camara con posicion, objetivo y arriba finitos, objetivo distinto de la posicion y vista y
/// vista-proyeccion finitas con el lente actual.
fn check_profile_pose(
    ship: &Ship,
    center: Vec3,
    chase_distance: f64,
    chase_height: f64,
    camera: &Camera,
) -> Result<(), String> {
    const TOLERANCE: f64 = 1e-9;
    let position = ship.position;
    let inside =
        |v: Vec3| v.is_finite() && v.x.abs().max(v.y.abs()).max(v.z.abs()) <= MAX_WORLD_COORD;
    let invalid = |what: &str| Err(format!("pose de perfil invalida: {what} ({position:?})"));
    if !(inside(position) && center.is_finite()) {
        return invalid("nave fuera de [-MAX_WORLD_COORD, MAX_WORLD_COORD] o no finita");
    }
    let basis = ship.orientation;
    let axes = [Vec3::X, Vec3::Y, Vec3::Z].map(|a| basis.transform_dir(a));
    let orthonormal = axes
        .iter()
        .all(|a| a.is_finite() && (a.length() - 1.0).abs() <= TOLERANCE)
        && axes[0].dot(axes[1]).abs() <= TOLERANCE
        && axes[0].dot(axes[2]).abs() <= TOLERANCE
        && axes[1].dot(axes[2]).abs() <= TOLERANCE;
    if !orthonormal || ship.forward().dot((center - position).normalized()) < 1.0 - TOLERANCE {
        return invalid("la nave no mira al centro con una base ortonormal");
    }
    let (back, up) = (axes[2], axes[1]);
    let eye = position + back * chase_distance + up * chase_height;
    let target = position - back * 30.0;
    let view = Mat4::look_at(eye, target, up);
    let usable = inside(eye)
        && target.is_finite()
        && (target - eye).length() > 0.0
        && view.m.iter().flatten().all(|v| v.is_finite())
        && (camera.projection() * view)
            .m
            .iter()
            .flatten()
            .all(|v| v.is_finite());
    if !usable {
        return invalid("la camara de persecucion resultante no es usable");
    }
    Ok(())
}

/// Cuerpo que sirve de Sol al preview solar: el sol animado, sin orbita, con radio finito
/// mayor que 0 y centro finito.
fn is_preview_sun(body: &CelestialBody, time: f64) -> bool {
    body.shader == ShaderKind::AnimatedSun
        && body.orbit.radius == 0.0
        && body.body_radius.is_finite()
        && body.body_radius > 0.0
        && body.position(time).is_finite()
}

/// Estrellas deterministas como direcciones al infinito: solo rota la vista, sin traslacion.
fn draw_stars(fb: &mut Framebuffer, camera: &Camera) {
    const COUNT: u32 = 700;
    let mut view = camera.view();
    for r in 0..3 {
        view.m[r][3] = 0.0;
    }
    let vp: Mat4 = camera.projection() * view;
    let hash = |n: u32| {
        let mut x = n.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        x as f64 / u32::MAX as f64
    };
    for i in 0..COUNT {
        let z = hash(i * 4) * 2.0 - 1.0;
        let a = hash(i * 4 + 1) * std::f64::consts::TAU;
        let r = (1.0 - z * z).sqrt();
        let dir = Vec3::new(r * a.cos(), z, r * a.sin());
        let c = vp.transform(dir.extend(1.0));
        if c.w <= 0.0 {
            continue;
        }
        let (x, y) = (
            (c.x / c.w * 0.5 + 0.5) * fb.width as f64,
            (0.5 - c.y / c.w * 0.5) * fb.height as f64,
        );
        let b = 0.35 + 0.65 * hash(i * 4 + 2);
        let color = to_rgb8(Vec3::new(b, b, b * (0.9 + 0.1 * hash(i * 4 + 3))));
        fb.write(x as isize, y as isize, 1.0, color);
    }
}

// ---- GT1-CARRY (aditivo; quitar este bloque restaura World byte a byte) ----

impl World {
    /// Commit VALIDADO de la pose de una nave capturada (preview de arrastre, opt-in): posicion finita y
    /// rotacion propia (columnas unitarias y ortogonales con 1e-9, det > 0, sin traslacion ni proyeccion).
    /// La nave se rehace con `Ship::new` y conserva orientacion y ajustes: velocidad e inercia angular a 0.
    /// La camara se refresca con la convencion de `update` (`desired_camera` con la base suavizada vigente,
    /// sin `snap`); el lente, el tiempo y la criatura no cambian. Con `Err` nada cambia.
    pub fn commit_captured_ship_pose(
        &mut self,
        position: Vec3,
        orientation: Mat4,
    ) -> Result<(), String> {
        let m = orientation.m;
        let cols = [0, 1, 2].map(|c| orientation.column(c));
        let proper = position.is_finite()
            && m.iter().flatten().all(|x| x.is_finite())
            && m[3] == [0.0, 0.0, 0.0, 1.0]
            && (0..3).all(|r| m[r][3] == 0.0)
            && cols.iter().all(|c| (c.length() - 1.0).abs() <= 1e-9)
            && cols[0].dot(cols[1]).abs() <= 1e-9
            && cols[0].dot(cols[2]).abs() <= 1e-9
            && cols[1].dot(cols[2]).abs() <= 1e-9
            && cols[0].cross(cols[1]).dot(cols[2]) > 0.0;
        if !proper {
            return Err(
                "pose de nave capturada no finita o no es una rotacion propia; nada cambia".into(),
            );
        }
        let mut ship = Ship::new(position, -Vec3::Z);
        ship.orientation = orientation;
        ship.settings = self.ship.settings;
        // Misma expresion que desired_camera, sobre el candidato: no publicar para validar.
        let back = self.camera_basis.transform_dir(Vec3::Z);
        let up = self.camera_basis.transform_dir(Vec3::Y);
        let camera_position = position + back * self.chase_distance + up * self.chase_height;
        let target = position - back * 30.0;
        let mut camera = self.camera;
        camera.position = camera_position;
        camera.target = target;
        camera.up = up;
        let forward = target - camera_position;
        let axes = [0, 1, 2].map(|c| self.camera_basis.column(c));
        let unit = |v: Vec3| v.is_finite() && (v.length() - 1.0).abs() <= 1e-9;
        let view = camera.view();
        let vp = camera.view_projection();
        let render = ship.transform(SHIP_MODEL_YAW).matrix();
        let mvp = vp * render;
        let finite = |m: &Mat4| m.m.iter().flatten().all(|x| x.is_finite());
        let usable = camera_position.is_finite()
            && target.is_finite()
            && axes.iter().all(|&a| unit(a))
            && axes[0].dot(axes[1]).abs() <= 1e-9
            && axes[0].dot(axes[2]).abs() <= 1e-9
            && axes[1].dot(axes[2]).abs() <= 1e-9
            && axes[0].cross(axes[1]).dot(axes[2]) > 0.0
            && forward.is_finite()
            && forward.length().is_finite()
            && forward.length() > 0.0
            && unit(forward.normalized().cross(up).normalized())
            && finite(&view)
            && (0..3).all(|r| unit(Vec3::new(view.m[r][0], view.m[r][1], view.m[r][2])))
            && finite(&vp)
            && finite(&render)
            && finite(&mvp)
            && Camera::new(camera.fov_y, camera.aspect, camera.near, camera.far).is_ok()
            && self.ship_model.vertices.iter().all(|&v| {
                let v = Vec3::from_array(v);
                let clip = mvp.transform(v.extend(1.0));
                render.transform_point(v).is_finite()
                    && [clip.x, clip.y, clip.z, clip.w]
                        .iter()
                        .all(|x| x.is_finite())
            });
        if !usable {
            return Err("camara o composicion de la nave capturada no usable; nada cambia".into());
        }
        self.ship = ship;
        self.camera = camera;
        self.gameplay.navigation.gravity_velocity = Vec3::ZERO;
        self.gameplay.navigation.assisted_planet = None;
        Ok(())
    }
}

// ---- GT3-AUTO (aditivo; quitar este bloque restaura el aprobado173 byte a byte) ----

impl World {
    /// `update_creature_eye_follow` con el CUERPO INMOVIL (GT3, cierre de la captura automatica): el ojo
    /// persigue su objetivo con los segundos reales como siempre, pero ni el giro global ni el seguimiento
    /// horizontal avanzan: su estado (yaw, objetivo, rapidez, activo) se conserva EXACTO, sin reinicio, y
    /// `update_creature_eye_follow` lo continua despues desde ese mismo estado. No toca nave, camara,
    /// tiempo, O2 ni revelacion. Sin mirada o sin criatura no hace nada.
    pub fn update_creature_eye_follow_body_held(&mut self, elapsed_seconds: f64) {
        let target = self.eye_target();
        if let (Some(target), Some(eye)) = (
            target,
            self.staged_reveal.as_mut().and_then(|m| m.eye.as_mut()),
        ) {
            eye.follow.update(target, elapsed_seconds);
        }
    }
}
