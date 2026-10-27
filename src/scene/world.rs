//! Mundo de juego: avanza la simulacion con `dt` y dibuja un cuadro completo.

use std::cell::{Cell, RefCell};

use super::{
    camera::Camera,
    ship::{Ship, ShipInput, smoothing},
    solar_system::SolarSystem,
};
use crate::{
    assets::{obj::Model, primitives::uv_sphere},
    math::{Mat4, Vec3},
    renderer::{
        clipping::Frustum,
        framebuffer::Framebuffer,
        pipeline::draw_model,
        shader::{Material, ShaderContext, ShaderKind, to_rgb8},
        stats::RenderStats,
    },
    visual::{
        creature_reveal::{CreatureReveal, RevealCompositor, RevealPart, RevealSettings},
        staged_creature_reveal::{EyeRegion, StagedParts, StagedReveal, StagedRevealSettings},
        static_creature::{AMBIENT, StaticCreature},
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
/// El OBJ de la nave apunta a +Z o -Z segun Blender; este giro lo alinea con -Z (D11).
pub const SHIP_MODEL_YAW: f64 = std::f64::consts::PI;

/// Interruptores de demostracion (UMB-404).
#[derive(Clone, Copy, Debug)]
pub struct DebugToggles {
    pub frustum_culling: bool,
    pub freeze_orbits: bool,
    pub lod: bool,
    pub backface_culling: bool,
}

impl Default for DebugToggles {
    fn default() -> Self {
        Self {
            frustum_culling: true,
            freeze_orbits: false,
            lod: true,
            backface_culling: true,
        }
    }
}

pub struct World {
    pub system: SolarSystem,
    pub ship: Ship,
    pub camera: Camera,
    pub ship_model: Model,
    /// Esferas de mas a menos detalle; ver `LOD_MIN_RATIO` y `World::lod_level`.
    pub sphere_lods: Vec<Model>,
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
}

/// Estado del modo por etapas: progreso, region del ojo y la particion en cache del modelo
/// de la criatura actual (`None` sin criatura).
struct StagedMode {
    reveal: StagedReveal,
    region: EyeRegion,
    parts: Option<StagedParts>,
}

impl World {
    pub fn new(ship_model: Model, aspect: f64) -> Result<Self, String> {
        let ship = Self::start_ship();
        let body_lod = vec![Cell::new(0); SolarSystem::demo().bodies.len()];
        let mut world = Self {
            system: SolarSystem::demo(),
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

    /// Restablece la nave; conserva el oxigeno y lleva la revelacion a su objetivo.
    pub fn reset_ship(&mut self) {
        self.ship = Self::start_ship();
        self.snap_camera();
        self.snap_creature_reveal();
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
        Ok(())
    }

    /// Posicion y objetivo de la camara: pegada a la nave (sin retraso de posicion) pero
    /// orientada con `camera_basis`, que sigue a la nave con suavizado. Asi la distancia
    /// camara-nave es constante a cualquier velocidad y solo la orientacion "se arrastra".
    fn desired_camera(&self) -> (Vec3, Vec3) {
        let basis = &self.camera_basis;
        let back = basis.transform_dir(Vec3::Z);
        let up = basis.transform_dir(Vec3::Y);
        let position = self.ship.position + back * self.chase_distance + up * self.chase_height;
        (position, self.ship.position - back * 30.0)
    }

    fn snap_camera(&mut self) {
        self.camera_basis = self.ship.orientation;
        let (position, target) = self.desired_camera();
        self.camera.position = position;
        self.camera.target = target;
        self.camera.up = self.camera_basis.transform_dir(Vec3::Y);
    }

    pub fn update(&mut self, input: &ShipInput, dt: f64) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        if !self.toggles.freeze_orbits {
            self.time += dt;
        }
        self.ship.update(input, dt);
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
        self.camera.up = self.camera_basis.transform_dir(Vec3::Y);
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

    /// Anade la criatura estatica (debug, opt-in). Comprueba antes que la luz del mundo sirve
    /// a su shader, para no descubrirlo al dibujar.
    pub fn set_creature(&mut self, creature: StaticCreature) -> Result<(), String> {
        TexturedShader::new(
            creature.texture(),
            TexturedMode::Lit,
            self.system.light_position(),
            AMBIENT,
        )?;
        // Con el modo por etapas, la nueva particion se calcula antes del cambio: si falla,
        // se devuelve el error y la criatura y la cache anteriores siguen intactas.
        let parts = self
            .staged_reveal
            .as_ref()
            .map(|m| StagedParts::split(creature.model(), &m.region))
            .transpose()?;
        if let Some(mode) = &mut self.staged_reveal {
            mode.parts = parts;
        }
        self.creature = Some(creature);
        self.snap_creature_reveal();
        Ok(())
    }
    /// Quita la criatura; con el modo por etapas suelta tambien su particion en cache.
    pub fn clear_creature(&mut self) -> Option<StaticCreature> {
        if let Some(mode) = &mut self.staged_reveal {
            mode.parts = None;
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
        Ok(())
    }

    fn context(&self) -> ShaderContext {
        ShaderContext {
            elapsed_time: self.time,
            oxygen01: self.debug_oxygen01.clamp(0.0, 1.0),
            camera_position: self.camera.position,
            ship_position: self.ship.position,
            light_position: self.system.light_position(),
        }
    }

    /// Dibuja el cuadro completo: fondo, estrellas, cuerpos, criatura (si la hay) y nave.
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
        if let Some(creature) = creature.filter(|_| opaque) {
            match creature.render(fb, &self.camera, creature_frustum, context.light_position) {
                Ok(drawn) => stats += drawn,
                // `set_creature` ya valido la luz y `light_position` es constante.
                Err(error) => debug_assert!(false, "criatura sin dibujar: {error}"),
            }
        }
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
            let drawn = match &self.staged_reveal {
                None => self.reveal_compositor.borrow_mut().render(
                    fb,
                    creature,
                    &self.camera,
                    creature_frustum,
                    context.light_position,
                    alpha,
                ),
                Some(mode) => self.render_staged(
                    fb,
                    creature,
                    mode.parts.as_ref(),
                    creature_frustum,
                    context.light_position,
                    (alpha, body_alpha),
                ),
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
    /// el transform de la criatura original. Cuenta **una** criatura logica y los vertices,
    /// triangulos y fragmentos que dibujan sus dos partes.
    fn render_staged(
        &self,
        fb: &mut Framebuffer,
        creature: &StaticCreature,
        parts: Option<&StagedParts>,
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
        debug_assert!(
            body_alpha <= 0.0 || eye_alpha >= 1.0,
            "cuerpo con ojo parcial"
        );
        let transform = creature.transform();
        let part = |model| RevealPart {
            model,
            transform: &transform,
            texture: creature.texture(),
        };
        let mut compositor = self.reveal_compositor.borrow_mut();
        let mut stats = compositor.render_part(
            fb,
            part(parts.eye()),
            &self.camera,
            light_position,
            eye_alpha,
        )?;
        if body_alpha > 0.0 {
            stats += compositor.render_part(
                fb,
                part(parts.body()),
                &self.camera,
                light_position,
                body_alpha,
            )?;
        }
        stats.objects_submitted = 1;
        Ok(stats)
    }
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
