//! Criatura estatica opcional en el juego (ruta debug, opt-in). El mundo la dibuja con
//! `draw_model_with` y `TexturedShader` en el mismo framebuffer, z-buffer y camara de la
//! nave; no usa el render del visor (que limpia el cuadro y crea su propia camara). La
//! escena por defecto no la tiene. Ver `docs/static-creature.md`.

use crate::{
    assets::obj::{Model, load_model},
    math::{Mat4, Transform, Vec3},
    renderer::{
        clipping::Frustum, framebuffer::Framebuffer, pipeline::draw_model_with, shader::Material,
        stats::RenderStats, texture::Texture,
    },
    scene::camera::Camera,
    visual::{
        creature_viewer::CreatureBounds,
        textured::{TexturedMode, TexturedShader},
    },
};
use std::{
    f64::consts::TAU,
    path::{Path, PathBuf},
};

/// Ubicacion de demostracion: fuera de todas las orbitas, frente original (+Z) hacia el Sol.
pub const DEMO_CENTER: Vec3 = Vec3::new(0.0, 0.0, -900.0);
/// Radio de la esfera envolvente en mundo.
pub const DEMO_WORLD_RADIUS: f64 = 80.0;
/// Fraccion de luz en sombra del shader texturizado (como el preview y el visor).
pub const AMBIENT: f64 = 0.25;
/// Contrato de colocacion: componentes del centro, radio en mundo.
pub const MAX_WORLD_COORD: f64 = 1e6;
pub const MIN_WORLD_RADIUS: f64 = 1e-3;
pub const MAX_WORLD_RADIUS: f64 = 1e5;
/// Distancia de la nave al centro en el arranque `--near-creature`, en radios.
pub const SPAWN_DISTANCE_RADII: f64 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreaturePlacement {
    center: Vec3,
    world_radius: f64,
    yaw: f64,
}

impl CreaturePlacement {
    /// `center` con componentes en `[-MAX_WORLD_COORD, MAX_WORLD_COORD]`, `world_radius` en
    /// `[MIN_WORLD_RADIUS, MAX_WORLD_RADIUS]` y `yaw` (radianes, eje Y) en `[-2pi, 2pi]`.
    pub fn new(center: Vec3, world_radius: f64, yaw: f64) -> Result<Self, String> {
        let largest = center.x.abs().max(center.y.abs()).max(center.z.abs());
        if !(center.is_finite() && largest <= MAX_WORLD_COORD) {
            return Err(format!(
                "centro de la criatura fuera de [-{MAX_WORLD_COORD}, {MAX_WORLD_COORD}]: {center:?}"
            ));
        }
        if !(MIN_WORLD_RADIUS..=MAX_WORLD_RADIUS).contains(&world_radius) {
            return Err(format!(
                "radio de la criatura fuera de [{MIN_WORLD_RADIUS}, {MAX_WORLD_RADIUS}]: {world_radius}"
            ));
        }
        if !(yaw.is_finite() && yaw.abs() <= TAU) {
            return Err(format!("yaw de la criatura fuera de [-2pi, 2pi]: {yaw}"));
        }
        Ok(Self {
            center,
            world_radius,
            yaw,
        })
    }
    /// Ubicacion de demostracion fuera de todas las orbitas, frente original hacia +Z (el
    /// Sol). No es la posicion final del nivel.
    pub fn demo() -> Self {
        Self {
            center: DEMO_CENTER,
            world_radius: DEMO_WORLD_RADIUS,
            yaw: 0.0,
        }
    }
    pub fn center(&self) -> Vec3 {
        self.center
    }
    pub fn world_radius(&self) -> f64 {
        self.world_radius
    }
    pub fn yaw(&self) -> f64 {
        self.yaw
    }
    /// Direccion del frente del OBJ (+Z) en mundo.
    pub fn front(&self) -> Vec3 {
        let (sin, cos) = self.yaw.sin_cos();
        Vec3::new(sin, 0.0, cos)
    }
    /// Posicion y direccion de la nave para ver la criatura de frente: del lado de `front()`,
    /// a `SPAWN_DISTANCE_RADII` radios del centro y mirando al centro.
    pub fn spawn_pose(&self) -> (Vec3, Vec3) {
        let front = self.front();
        (
            self.center + front * (SPAWN_DISTANCE_RADII * self.world_radius),
            -front,
        )
    }
    /// Escala uniforme `world_radius / model_radius`, giro `yaw` y traslacion que lleva el
    /// centro envolvente del modelo a `center`. Los bounds del modelo siguen el contrato de
    /// `CreatureBounds`; `Err` si algo no es finito.
    pub fn transform_for(
        &self,
        model_center: Vec3,
        model_radius: f64,
    ) -> Result<Transform, String> {
        let bounds = CreatureBounds::new(model_center, model_radius)
            .map_err(|e| format!("bounds del modelo invalidos: {e}"))?;
        let scale = self.world_radius / bounds.radius();
        let rotated = Mat4::rotation_y(self.yaw).transform_dir(bounds.center() * scale);
        let transform = Transform {
            position: self.center - rotated,
            rotation: Vec3::new(0.0, self.yaw, 0.0),
            scale: Vec3::splat(scale),
        };
        let finite = transform.matrix().m.iter().flatten().all(|v| v.is_finite());
        if !(scale.is_finite() && finite) {
            return Err(format!("transform no finito para {self:?}"));
        }
        Ok(transform)
    }
}

pub struct StaticCreature {
    model: Model,
    texture: Texture,
    placement: CreaturePlacement,
    transform: Transform,
}

impl StaticCreature {
    /// Toma posesion del modelo y la textura; el transform se calcula aqui una vez.
    pub fn new(
        model: Model,
        texture: Texture,
        placement: CreaturePlacement,
    ) -> Result<Self, String> {
        let transform =
            placement.transform_for(Vec3::from_array(model.bounds_center), model.bounds_radius)?;
        Ok(Self {
            model,
            texture,
            placement,
            transform,
        })
    }
    pub fn load(
        model_path: &Path,
        texture_path: &Path,
        placement: CreaturePlacement,
    ) -> Result<Self, String> {
        let model = load_model(model_path)
            .map_err(|e| format!("Criatura, modelo {}: {e}", model_path.display()))?;
        let texture = Texture::load_png(texture_path)
            .map_err(|e| format!("Criatura, textura {}: {e}", texture_path.display()))?;
        Self::new(model, texture, placement)
    }
    pub fn model(&self) -> &Model {
        &self.model
    }
    pub fn texture(&self) -> &Texture {
        &self.texture
    }
    pub fn placement(&self) -> CreaturePlacement {
        self.placement
    }
    pub fn transform(&self) -> Transform {
        self.transform
    }
    /// Dibuja sobre el cuadro actual sin limpiarlo, con la camara y la luz del mundo, en modo
    /// `Lit`. Con `frustum` (descarte activo) una criatura fuera de vista solo se cuenta, sin
    /// transformar vertices. Siempre a doble cara: la malla de la criatura no es cerrada.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        camera: &Camera,
        frustum: Option<&Frustum>,
        light_position: Vec3,
    ) -> Result<RenderStats, String> {
        let (center, radius) = (self.placement.center, self.placement.world_radius);
        if frustum.is_some_and(|f| !f.sphere_visible(center, radius)) {
            let faces = self.model.faces.len();
            return Ok(RenderStats {
                objects_submitted: 1,
                objects_culled: 1,
                triangles_submitted: faces,
                triangles_culled: faces,
                ..RenderStats::default()
            });
        }
        let shader =
            TexturedShader::new(&self.texture, TexturedMode::Lit, light_position, AMBIENT)?;
        Ok(draw_model_with(
            framebuffer,
            &self.model,
            &self.transform,
            &Material::from_model(),
            camera,
            |fragment, _| shader.shade(fragment),
        ))
    }
}

/// Criatura pedida por la linea de comandos del juego.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureOptions {
    pub model: PathBuf,
    pub texture: PathBuf,
    /// Arrancar (y restablecer con `R`) la nave frente a la criatura.
    pub near: bool,
}

impl CreatureOptions {
    /// Valida los flags de la criatura sin tocar archivos. Sin ninguno devuelve `None` (juego
    /// normal). `near_body` es `--near N`; `legacy`, `--legacy`.
    pub fn from_flags(
        model: Option<PathBuf>,
        texture: Option<PathBuf>,
        near_creature: bool,
        near_body: Option<usize>,
        legacy: bool,
    ) -> Result<Option<Self>, String> {
        let options = match (model, texture) {
            (Some(model), Some(texture)) => Some(Self {
                model,
                texture,
                near: near_creature,
            }),
            (None, None) => None,
            _ => return Err("--creature-model y --creature-texture deben usarse juntos".into()),
        };
        if legacy && (options.is_some() || near_creature) {
            return Err(
                "--legacy no admite --creature-model, --creature-texture ni --near-creature".into(),
            );
        }
        if near_creature && options.is_none() {
            return Err("--near-creature necesita --creature-model y --creature-texture".into());
        }
        if near_creature && near_body.is_some() {
            return Err(
                "--near y --near-creature son incompatibles: elige un solo arranque".into(),
            );
        }
        Ok(options)
    }
}
