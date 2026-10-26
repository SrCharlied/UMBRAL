//! Visor orbital de la criatura (vista tecnica aislada, CPU). Estado de camara puro y
//! comprobable, y un unico render compartido por el modo headless y la ventana del example
//! `creature_viewer`. La criatura es estatica: solo se mueve la camara. La escena del juego
//! no lo usa. Ver `docs/creature-viewer.md`.

use crate::{
    assets::obj::Model,
    math::{Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer, pipeline::draw_model_with, shader::Material, stats::RenderStats,
        texture::Texture,
    },
    scene::camera::Camera,
    visual::textured::{TexturedMode, TexturedShader},
};
use std::{
    f64::consts::{PI, TAU},
    path::Path,
};

/// Mismo lente, encuadre y luz que el preview estatico (`examples/creature_preview.rs`).
pub const FOV_DEGREES: f64 = 40.0;
pub const FRAME_MARGIN: f64 = 1.1;
pub const AMBIENT: f64 = 0.25;
/// Plano cercano y lejano en proporcion a la distancia de la camara al centro.
const NEAR_FRACTION: f64 = 0.01;
const FAR_FACTOR: f64 = 4.0;
/// Tope de presentacion de la ventana (`set_target_fps`).
pub const TARGET_FPS: usize = 60;

/// Distancia de la camara al centro envolvente, en radios: siempre fuera de la esfera.
pub const MIN_DISTANCE_RADII: f64 = 1.25;
pub const MAX_DISTANCE_RADII: f64 = 12.0;
/// Elevacion maxima en valor absoluto (~80 grados): la vista nunca es paralela al eje Y.
pub const MAX_PITCH: f64 = 1.4;
/// Radianes por segundo con la flecha pulsada.
pub const YAW_RATE: f64 = 1.5;
pub const PITCH_RATE: f64 = 1.0;
/// Acercamiento exponencial: la distancia se multiplica por `e^(-ZOOM_RATE * dt)` con `W`.
pub const ZOOM_RATE: f64 = 1.0;
/// Paso maximo de tiempo: una pausa larga avanza como un cuadro de `MAX_DT` segundos.
pub const MAX_DT: f64 = 0.1;

/// Contrato de la esfera envolvente: componentes del centro en `[-MAX_CENTER_COORD,
/// MAX_CENTER_COORD]`, radio en `[MIN_RADIUS, MAX_RADIUS]` y no menor que
/// `MIN_RADIUS_TO_CENTER` por la mayor componente del centro (precision f64 suficiente).
pub const MAX_CENTER_COORD: f64 = 1e9;
pub const MIN_RADIUS: f64 = 1e-6;
pub const MAX_RADIUS: f64 = 1e9;
pub const MIN_RADIUS_TO_CENTER: f64 = 1e-9;

/// Envuelve un yaw finito a `[-pi, pi]`; dentro del rango lo devuelve sin redondeo.
fn wrap_yaw(yaw: f64) -> f64 {
    if (-PI..=PI).contains(&yaw) {
        yaw
    } else {
        (yaw + PI).rem_euclid(TAU) - PI
    }
}

/// Pose orbital valida: yaw envuelto, `|pitch| <= MAX_PITCH` y distancia en radios en
/// `[MIN_DISTANCE_RADII, MAX_DISTANCE_RADII]`. Solo se construye validada.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitPose {
    yaw: f64,
    pitch: f64,
    distance_radii: f64,
}

impl OrbitPose {
    /// Angulos en radianes. Yaw 0 y pitch 0 miran desde +Z del OBJ; yaw positivo lleva la
    /// camara hacia +X y pitch positivo la eleva. `Err` con NaN, infinitos o fuera de rango.
    pub fn new(yaw: f64, pitch: f64, distance_radii: f64) -> Result<Self, String> {
        if !yaw.is_finite() {
            return Err(format!("yaw debe ser finito: {yaw}"));
        }
        if !(pitch.is_finite() && pitch.abs() <= MAX_PITCH) {
            return Err(format!(
                "pitch debe estar en [-{MAX_PITCH}, {MAX_PITCH}] radianes: {pitch}"
            ));
        }
        if !(MIN_DISTANCE_RADII..=MAX_DISTANCE_RADII).contains(&distance_radii) {
            return Err(format!(
                "distancia debe estar en [{MIN_DISTANCE_RADII}, {MAX_DISTANCE_RADII}] radios: {distance_radii}"
            ));
        }
        Ok(Self {
            yaw: wrap_yaw(yaw),
            pitch,
            distance_radii,
        })
    }
    pub fn yaw(&self) -> f64 {
        self.yaw
    }
    pub fn pitch(&self) -> f64 {
        self.pitch
    }
    pub fn distance_radii(&self) -> f64 {
        self.distance_radii
    }
    /// Direccion unitaria del centro envolvente hacia la camara.
    pub fn direction(&self) -> Vec3 {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(cos_pitch * sin_yaw, sin_pitch, cos_pitch * cos_yaw)
    }
}

impl Default for OrbitPose {
    /// Encuadre del preview estatico: frente (+Z), ligeramente elevado, la esfera envolvente
    /// cabe en el FOV vertical con `FRAME_MARGIN`.
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.15_f64.atan(),
            distance_radii: FRAME_MARGIN / (FOV_DEGREES.to_radians() / 2.0).sin(),
        }
    }
}

/// Ejes de control en `[-1, 1]`: `yaw` (+ derecha), `pitch` (+ arriba), `zoom` (+ acercar).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitInput {
    pub yaw: f64,
    pub pitch: f64,
    pub zoom: f64,
}

impl OrbitInput {
    pub const NONE: Self = Self {
        yaw: 0.0,
        pitch: 0.0,
        zoom: 0.0,
    };
    /// Teclas opuestas pulsadas a la vez se anulan.
    pub fn from_keys(
        left: bool,
        right: bool,
        up: bool,
        down: bool,
        closer: bool,
        farther: bool,
    ) -> Self {
        let axis = |plus: bool, minus: bool| f64::from(i8::from(plus) - i8::from(minus));
        Self {
            yaw: axis(right, left),
            pitch: axis(up, down),
            zoom: axis(closer, farther),
        }
    }
}

/// Estado de la camara orbital. No guarda ni modifica nada de la criatura.
#[derive(Clone, Copy, Debug)]
pub struct OrbitController {
    initial: OrbitPose,
    pose: OrbitPose,
}

impl OrbitController {
    pub fn new(initial: OrbitPose) -> Self {
        Self {
            initial,
            pose: initial,
        }
    }
    pub fn pose(&self) -> OrbitPose {
        self.pose
    }
    /// Vuelve exactamente a la pose con la que se creo el controlador.
    pub fn reset(&mut self) {
        self.pose = self.initial;
    }
    /// Avanza `dt` segundos (finito y no negativo; se recorta a `MAX_DT`). Pitch y distancia
    /// se saturan en sus limites. Con `Err` el estado no cambia.
    pub fn step(&mut self, input: OrbitInput, dt: f64) -> Result<(), String> {
        if !(dt.is_finite() && dt >= 0.0) {
            return Err(format!("dt debe ser finito y no negativo: {dt}"));
        }
        for value in [input.yaw, input.pitch, input.zoom] {
            if !(value.is_finite() && value.abs() <= 1.0) {
                return Err(format!(
                    "los ejes de control deben estar en [-1, 1]: {input:?}"
                ));
            }
        }
        let dt = dt.min(MAX_DT);
        let pose = OrbitPose {
            yaw: wrap_yaw(self.pose.yaw + input.yaw * YAW_RATE * dt),
            pitch: (self.pose.pitch + input.pitch * PITCH_RATE * dt).clamp(-MAX_PITCH, MAX_PITCH),
            distance_radii: (self.pose.distance_radii * (-input.zoom * ZOOM_RATE * dt).exp())
                .clamp(MIN_DISTANCE_RADII, MAX_DISTANCE_RADII),
        };
        self.pose = pose;
        Ok(())
    }
}

/// Esfera envolvente validada de la criatura (espacio de objeto = mundo: la criatura no se
/// transforma).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreatureBounds {
    center: Vec3,
    radius: f64,
}

impl CreatureBounds {
    pub fn new(center: Vec3, radius: f64) -> Result<Self, String> {
        let largest = center.x.abs().max(center.y.abs()).max(center.z.abs());
        if !(center.is_finite() && largest <= MAX_CENTER_COORD) {
            return Err(format!(
                "centro envolvente fuera de [-{MAX_CENTER_COORD}, {MAX_CENTER_COORD}]: {center:?}"
            ));
        }
        if !(MIN_RADIUS..=MAX_RADIUS).contains(&radius) {
            return Err(format!(
                "radio envolvente fuera de [{MIN_RADIUS}, {MAX_RADIUS}]: {radius}"
            ));
        }
        if radius < largest * MIN_RADIUS_TO_CENTER {
            return Err(format!(
                "radio envolvente {radius} demasiado pequeno para un centro tan desplazado ({center:?})"
            ));
        }
        Ok(Self { center, radius })
    }
    pub fn from_model(model: &Model) -> Result<Self, String> {
        Self::new(Vec3::from_array(model.bounds_center), model.bounds_radius)
    }
    pub fn center(&self) -> Vec3 {
        self.center
    }
    pub fn radius(&self) -> f64 {
        self.radius
    }
    /// Camara que mira al centro desde `pose`; `Err` si el aspecto es invalido o alguna
    /// salida no es finita.
    pub fn camera(&self, pose: &OrbitPose, aspect: f64) -> Result<Camera, String> {
        let distance = pose.distance_radii * self.radius;
        let mut camera = Camera::new(
            FOV_DEGREES.to_radians(),
            aspect,
            distance * NEAR_FRACTION,
            distance * FAR_FACTOR,
        )?;
        camera.position = self.center + pose.direction() * distance;
        camera.target = self.center;
        let finite = camera.position.is_finite()
            && camera
                .view_projection()
                .m
                .iter()
                .flatten()
                .all(|v| v.is_finite());
        if !finite {
            return Err(format!("camara no finita para {pose:?} y {self:?}"));
        }
        Ok(camera)
    }
    /// Luz fija en el mundo (arriba, a la izquierda y por delante), como el preview estatico.
    pub fn light_position(&self) -> Vec3 {
        self.center + Vec3::new(-1.5, 2.0, 3.0) * self.radius
    }
}

/// Render unico del visor (headless y ventana): limpia el framebuffer y dibuja `model` sin
/// transformar con `draw_model_with` y `TexturedShader`, sin descartar caras traseras (la
/// malla de la criatura no es cerrada). Con `Err` el framebuffer no se toca.
pub fn render_view(
    framebuffer: &mut Framebuffer,
    model: &Model,
    texture: &Texture,
    bounds: &CreatureBounds,
    pose: &OrbitPose,
    mode: TexturedMode,
) -> Result<RenderStats, String> {
    let aspect = framebuffer.width as f64 / framebuffer.height as f64;
    let camera = bounds.camera(pose, aspect)?;
    let shader = TexturedShader::new(texture, mode, bounds.light_position(), AMBIENT)?;
    framebuffer.clear();
    Ok(draw_model_with(
        framebuffer,
        model,
        &Transform::IDENTITY,
        &Material::from_model(),
        &camera,
        |fragment, _| shader.shade(fragment),
    ))
}

/// Tiempos de la ventana: CPU del render (sin HUD ni presentacion) y cuadros presentados
/// por segundo (incluye la espera del tope `TARGET_FPS`).
#[derive(Clone, Copy, Debug)]
pub struct HudTiming {
    pub render_ms: f64,
    pub presented_fps: f64,
}

fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.1}")
    } else {
        "--".into()
    }
}

/// Lineas del HUD. Sin `timing` (headless) el texto solo depende de sus argumentos.
pub fn hud_lines(
    mode: TexturedMode,
    texture_size: (u32, u32),
    pose: &OrbitPose,
    timing: Option<HudTiming>,
) -> Vec<String> {
    let mode = match mode {
        TexturedMode::Lit => "LIT",
        TexturedMode::Unlit => "UNLIT",
    };
    let mut lines = vec![format!(
        "VISOR TECNICO | MODO {mode} | TEXTURA {}X{} | DISTANCIA {:.2} RADIOS",
        texture_size.0, texture_size.1, pose.distance_radii
    )];
    if let Some(timing) = timing {
        lines.push(format!(
            "RENDER CPU {} MS | PRESENTADOS {} FPS (TOPE {TARGET_FPS})",
            number(timing.render_ms),
            number(timing.presented_fps)
        ));
    }
    lines.push(
        "FLECHAS ORBITAR | W/S ACERCAR/ALEJAR | R RESET | L LIT/UNLIT | P CAPTURA | ESC SALIR"
            .into(),
    );
    lines
}

/// `output` debe terminar en `.png` y no puede ser (tras resolver la ruta) ninguna de las
/// entradas. Un archivo que aun no existe no puede coincidir con una entrada existente.
pub fn validate_output_path(output: &Path, inputs: &[&Path]) -> Result<(), String> {
    let is_png = output
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if !is_png {
        return Err(format!(
            "--output debe terminar en .png: {}",
            output.display()
        ));
    }
    if !output.exists() {
        return Ok(());
    }
    let resolve = |path: &Path| {
        std::fs::canonicalize(path)
            .map_err(|e| format!("No se pudo resolver {}: {e}", path.display()))
    };
    let resolved = resolve(output)?;
    for input in inputs {
        if resolve(input)? == resolved {
            return Err(format!(
                "--output {} coincide con la entrada {}",
                output.display(),
                input.display()
            ));
        }
    }
    Ok(())
}
