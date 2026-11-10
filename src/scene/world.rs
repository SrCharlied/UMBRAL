//! Mundo de juego: avanza la simulacion con `dt` y dibuja un cuadro completo.

use std::cell::Cell;

use super::{
    camera::Camera,
    ship::{Ship, ShipInput, smoothing},
    solar_system::SolarSystem,
};
use crate::{
    assets::{obj::Model, primitives::uv_sphere},
    math::{Mat4, Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer,
        pipeline::draw_model,
        shader::{Material, ShaderContext, ShaderKind, to_rgb8},
        stats::RenderStats,
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
    pub gameplay: crate::gameplay::GameplayState,
    pub orbit_assist: bool,
    pub forced_oxygen: Option<f64>,
    mirrored_oxygen: f64,
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

    pub fn reset_ship(&mut self) {
        let settings = self.gameplay.settings;
        self.gameplay = crate::gameplay::GameplayState::default();
        self.gameplay.settings = settings;
        self.time = 0.0;
        self.orbit_assist = false;
        self.forced_oxygen = None;
        self.debug_oxygen01 = 1.0;
        self.mirrored_oxygen = 1.0;
        self.ship = Self::start_ship();
        self.snap_camera();
    }

    /// Oxigeno visual: override explicito, control legado, o valor real.
    pub fn oxygen01(&self) -> f64 {
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
            self.forced_oxygen = Some(value.clamp(0.0, 1.0));
            self.debug_oxygen01 = value.clamp(0.0, 1.0);
            self.mirrored_oxygen = self.debug_oxygen01;
        }
    }

    pub fn clear_debug_oxygen(&mut self) {
        self.forced_oxygen = None;
        self.debug_oxygen01 = self.gameplay.oxygen01;
        self.mirrored_oxygen = self.debug_oxygen01;
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
        let previous = self.ship.position;
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
        self.debug_oxygen01 = self.forced_oxygen.unwrap_or(self.gameplay.oxygen01);
        self.mirrored_oxygen = self.debug_oxygen01;
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
        stats
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
