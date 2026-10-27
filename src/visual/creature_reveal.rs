//! Revelacion de la criatura por oxigeno (debug, opt-in): estado puro y compositor fantasma.
//! Ver `docs/static-creature.md`.

use crate::{
    assets::obj::Model,
    math::{Transform, Vec3},
    renderer::{
        clipping::Frustum,
        framebuffer::Framebuffer,
        pipeline::draw_model_with,
        shader::{Material, to_rgb8},
        stats::RenderStats,
        texture::Texture,
    },
    scene::camera::Camera,
    visual::{
        static_creature::{AMBIENT, StaticCreature},
        textured::{TexturedMode, TexturedShader},
    },
};
use std::sync::OnceLock;

/// Paso manual del medidor de oxigeno de depuracion: M aumenta y N reduce cinco puntos.
/// Al pulsar se usan porcentajes enteros (25.3 % parte de 25 %) para evitar deriva de coma
/// flotante en los umbrales. Sin pulsacion o con ambas teclas conserva el valor acotado.
/// Un oxigeno no finito vuelve al 100 % (politica defensiva).
pub fn step_debug_oxygen(oxygen01: f64, increase: bool, decrease: bool) -> f64 {
    let current = if oxygen01.is_finite() {
        oxygen01.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let direction = i32::from(increase) - i32::from(decrease);
    if direction == 0 {
        return current;
    }
    let percent = (current * 100.0).round() as i32;
    f64::from((percent + direction * 5).clamp(0, 100)) / 100.0
}

/// Valor de `--debug-oxygen`: un numero finito en `[0, 1]`.
pub fn parse_debug_oxygen(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|v| (0.0..=1.0).contains(v))
        .ok_or_else(|| format!("--debug-oxygen necesita un valor entre 0 y 1: {text:?}"))
}

/// Combinaciones de `--creature-reveal` y `--debug-oxygen`, sin tocar archivos. Se valida
/// despues de `CreatureOptions::from_flags`.
pub fn validate_reveal_flags(
    reveal: bool,
    has_creature: bool,
    debug_oxygen_given: bool,
    legacy: bool,
) -> Result<(), String> {
    if legacy && (reveal || debug_oxygen_given) {
        return Err("--legacy no admite --creature-reveal ni --debug-oxygen".into());
    }
    if reveal && !has_creature {
        return Err("--creature-reveal necesita --creature-model y --creature-texture".into());
    }
    Ok(())
}

/// Limites de los tiempos de transicion (segundos hasta el 95 %).
pub const MIN_TRANSITION_SECONDS: f64 = 0.01;
pub const MAX_TRANSITION_SECONDS: f64 = 60.0;

/// Ajustes de la revelacion. Solo se construyen validados.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RevealSettings {
    start_oxygen: f64,
    full_oxygen: f64,
    rise_seconds: f64,
    fall_seconds: f64,
}

impl RevealSettings {
    /// `0 <= full_oxygen < start_oxygen <= 1`: por encima de `start_oxygen` la criatura esta
    /// oculta y desde `full_oxygen` hacia abajo, entera. `rise_seconds` y `fall_seconds` son
    /// los segundos reales para recorrer el 95 % de un cambio al aparecer y al ocultarse, en
    /// `[MIN_TRANSITION_SECONDS, MAX_TRANSITION_SECONDS]`.
    pub fn new(
        start_oxygen: f64,
        full_oxygen: f64,
        rise_seconds: f64,
        fall_seconds: f64,
    ) -> Result<Self, String> {
        if !(0.0 <= full_oxygen && full_oxygen < start_oxygen && start_oxygen <= 1.0) {
            return Err(format!(
                "umbrales de oxigeno invalidos: se necesita 0 <= {full_oxygen} < {start_oxygen} <= 1"
            ));
        }
        let seconds = MIN_TRANSITION_SECONDS..=MAX_TRANSITION_SECONDS;
        if !(seconds.contains(&rise_seconds) && seconds.contains(&fall_seconds)) {
            return Err(format!(
                "tiempos de transicion fuera de [{MIN_TRANSITION_SECONDS}, {MAX_TRANSITION_SECONDS}] s: {rise_seconds} {fall_seconds}"
            ));
        }
        Ok(Self {
            start_oxygen,
            full_oxygen,
            rise_seconds,
            fall_seconds,
        })
    }
    pub fn start_oxygen(&self) -> f64 {
        self.start_oxygen
    }
    pub fn full_oxygen(&self) -> f64 {
        self.full_oxygen
    }
    pub fn rise_seconds(&self) -> f64 {
        self.rise_seconds
    }
    pub fn fall_seconds(&self) -> f64 {
        self.fall_seconds
    }
}

impl Default for RevealSettings {
    /// Aviso (0.50) y critico (0.10) de UMB-310; aparece en ~2 s y se oculta en ~0.7 s.
    fn default() -> Self {
        Self {
            start_oxygen: 0.50,
            full_oxygen: 0.10,
            rise_seconds: 2.0,
            fall_seconds: 0.7,
        }
    }
}

/// Visibilidad objetivo en `[0, 1]` para un oxigeno: `smoothstep` continuo de
/// `start_oxygen` (0) a `full_oxygen` (1). Fuera de `[0, 1]` el oxigeno se recorta; uno no
/// finito se trata como "sin dato" y deja la criatura oculta (politica defensiva).
pub fn reveal_target(oxygen01: f64, settings: &RevealSettings) -> f64 {
    if !oxygen01.is_finite() {
        return 0.0;
    }
    let span = settings.start_oxygen - settings.full_oxygen;
    let t = ((settings.start_oxygen - oxygen01.clamp(0.0, 1.0)) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Estado del efecto: solo los ajustes y la visibilidad mostrada en `[0, 1]` (0 = no se
/// dibuja, 1 = ruta estatica exacta). No guarda tiempo absoluto: el oxigeno y el tiempo real
/// transcurrido llegan como argumentos.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreatureReveal {
    settings: RevealSettings,
    shown: f64,
}

impl CreatureReveal {
    /// Arranca ya en el objetivo de `oxygen01`, sin transicion.
    pub fn new(settings: RevealSettings, oxygen01: f64) -> Self {
        Self {
            settings,
            shown: reveal_target(oxygen01, &settings),
        }
    }
    pub fn settings(&self) -> RevealSettings {
        self.settings
    }
    pub fn shown(&self) -> f64 {
        self.shown
    }
    pub fn target(&self, oxygen01: f64) -> f64 {
        reveal_target(oxygen01, &self.settings)
    }
    /// Avanza `elapsed` segundos reales hacia el objetivo de `oxygen01` (`step_reveal`).
    pub fn update(&mut self, oxygen01: f64, elapsed: f64) {
        self.shown = step_reveal(self.shown, self.target(oxygen01), elapsed, &self.settings);
    }
    /// Salta al objetivo de `oxygen01` sin transicion.
    pub fn snap(&mut self, oxygen01: f64) {
        self.shown = self.target(oxygen01);
    }
}

/// Compositor "fantasma" de una sola capa para la revelacion parcial (`0 < alpha < 1`).
///
/// 1. Copia los pixeles del cuadro ya dibujado (cuerpos, estrellas y nave): el fondo real.
/// 2. Dibuja la criatura opaca e iluminada encima con el z-buffer real y anota los pixeles
///    cuyos fragmentos aceptan el z-test. En `triangle_shaded` sombrear implica escribir, asi
///    que al final cada pixel anotado tiene el color de la superficie de la criatura mas
///    cercana.
/// 3. Mezcla una sola vez por pixel anotado, en lineal: `fondo + (criatura - fondo) * alpha`.
///
/// **Limites:** no es transparencia general.
/// - Solo se ve la superficie de la criatura mas cercana; sus capas internas no se ven a
///   traves de ella.
/// - El z-buffer queda con la profundidad de la criatura en esos pixeles, asi que tiene que
///   ser lo **ultimo** que se dibuja en 3D en el cuadro. Una entidad futura dibujada despues
///   la trataria como opaca.
///
/// Los buffers se reutilizan entre cuadros y se ajustan si cambia la resolucion. Los
/// extremos (0, 1) y una criatura descartada por el frustum no los tocan.
#[derive(Debug, Default)]
pub struct RevealCompositor {
    /// Copia del cuadro antes de dibujar la criatura.
    background: Vec<u32>,
    /// Pixeles ya anotados en este cuadro; siempre todo `false` entre cuadros.
    covered: Vec<bool>,
    /// Indices de los pixeles anotados, sin repetir.
    mask: Vec<u32>,
}

impl RevealCompositor {
    pub fn new() -> Self {
        Self::default()
    }
    /// Memoria reservada por los buffers de trabajo, en bytes.
    pub fn buffer_bytes(&self) -> usize {
        self.background.capacity() * size_of::<u32>()
            + self.covered.capacity() * size_of::<bool>()
            + self.mask.capacity() * size_of::<u32>()
    }
    /// Dibuja `creature` con visibilidad `alpha` sobre el cuadro actual. Con `alpha <= 0` o
    /// NaN no hace nada ni cuenta; con `alpha >= 1` delega en `StaticCreature::render`
    /// (identico a la estatica); descartada por `frustum`, solo se cuenta, como la estatica.
    /// Los contadores son los de dibujar la criatura: la mezcla no suma escrituras.
    pub fn render(
        &mut self,
        framebuffer: &mut Framebuffer,
        creature: &StaticCreature,
        camera: &Camera,
        frustum: Option<&Frustum>,
        light_position: Vec3,
        alpha: f64,
    ) -> Result<RenderStats, String> {
        if alpha.is_nan() || alpha <= 0.0 {
            return Ok(RenderStats::default());
        }
        let placement = creature.placement();
        let culled = frustum
            .is_some_and(|f| !f.sphere_visible(placement.center(), placement.world_radius()));
        if alpha >= 1.0 || culled {
            return creature.render(framebuffer, camera, frustum, light_position);
        }
        let transform = creature.transform();
        let part = RevealPart {
            model: creature.model(),
            transform: &transform,
            texture: creature.texture(),
        };
        self.composite(framebuffer, part, camera, light_position, alpha)
    }

    /// Como `render`, pero para una parte prestada (modelo, transform y textura), sin
    /// descarte por frustum: lo decide quien llama con la esfera de la criatura completa.
    /// Alfa 0 o NaN no dibuja ni cuenta; alfa 1 dibuja opaco igual que
    /// `StaticCreature::render` (Lit, ambiente `AMBIENT`, doble cara); parcial, el fantasma.
    pub fn render_part(
        &mut self,
        framebuffer: &mut Framebuffer,
        part: RevealPart<'_>,
        camera: &Camera,
        light_position: Vec3,
        alpha: f64,
    ) -> Result<RenderStats, String> {
        if alpha.is_nan() || alpha <= 0.0 {
            return Ok(RenderStats::default());
        }
        if alpha >= 1.0 {
            let shader =
                TexturedShader::new(part.texture, TexturedMode::Lit, light_position, AMBIENT)?;
            return Ok(draw_model_with(
                framebuffer,
                part.model,
                part.transform,
                &Material::from_model(),
                camera,
                |fragment, _| shader.shade(fragment),
            ));
        }
        self.composite(framebuffer, part, camera, light_position, alpha)
    }

    /// Fantasma de una capa (`0 < alpha < 1`): copia, dibuja con mascara y mezcla una vez.
    fn composite(
        &mut self,
        framebuffer: &mut Framebuffer,
        part: RevealPart<'_>,
        camera: &Camera,
        light_position: Vec3,
        alpha: f64,
    ) -> Result<RenderStats, String> {
        let shader = TexturedShader::new(part.texture, TexturedMode::Lit, light_position, AMBIENT)?;
        let pixels = framebuffer.pixels.len();
        self.background.clear();
        self.background.extend_from_slice(&framebuffer.pixels);
        if self.covered.len() != pixels {
            self.covered.clear();
            self.covered.resize(pixels, false);
        }
        self.mask.clear();
        let width = framebuffer.width;
        let (covered, mask) = (&mut self.covered, &mut self.mask);
        let stats = draw_model_with(
            framebuffer,
            part.model,
            part.transform,
            &Material::from_model(),
            camera,
            |fragment, _| {
                // Centro del pixel (x + 0.5, y + 0.5): truncar da el pixel.
                let p = fragment.screen_position;
                let index = p.y as usize * width + p.x as usize;
                if !covered[index] {
                    covered[index] = true;
                    mask.push(index as u32);
                }
                shader.shade(fragment)
            },
        );
        for &index in &self.mask {
            let index = index as usize;
            framebuffer.pixels[index] =
                blend_rgb8(self.background[index], framebuffer.pixels[index], alpha);
            self.covered[index] = false;
        }
        Ok(stats)
    }
}

/// Vista prestada de lo que dibuja el compositor: un modelo (o una parte de el), su
/// transform y su textura. No copia nada; la textura sigue siendo la de la criatura.
#[derive(Clone, Copy)]
pub struct RevealPart<'a> {
    pub model: &'a Model,
    pub transform: &'a Transform,
    pub texture: &'a Texture,
}

/// Canal sRGB de 8 bits a lineal con la curva sRGB estandar: inversa exacta de `to_rgb8`
/// (`to_rgb8` de este valor devuelve el mismo codigo, comprobado para los 256).
pub fn srgb8_to_linear(code: u8) -> f64 {
    static TABLE: OnceLock<[f64; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let s = i as f64 / 255.0;
            if s <= 0.040_45 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        })
    })[usize::from(code)]
}

/// Mezcla `background` y `foreground` (`0xRRGGBB` sRGB) en luz lineal, una sola vez:
/// `alpha` 0 da el fondo y 1 el frente, exactos. `alpha` se recorta a `[0, 1]`; no finito
/// deja el fondo (politica defensiva).
pub fn blend_rgb8(background: u32, foreground: u32, alpha: f64) -> u32 {
    let alpha = if alpha.is_finite() {
        alpha.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let channel = |shift: u32| {
        let b = srgb8_to_linear((background >> shift) as u8);
        let f = srgb8_to_linear((foreground >> shift) as u8);
        b + (f - b) * alpha
    };
    to_rgb8(Vec3::new(channel(16), channel(8), channel(0)))
}

/// Distancia al objetivo por debajo de la cual el valor mostrado se fija exactamente en el.
/// Sin esto la persecucion exponencial nunca llegaria a 0 o a 1, y los extremos (sin dibujar
/// y ruta estatica exacta) no se alcanzarian. Con los ajustes por defecto, ir de 0 a 1 tarda
/// ~4.6 s y de 1 a 0 ~1.6 s (`segundos_95 * ln(1000) / ln(20)`).
pub const REVEAL_SNAP_EPSILON: f64 = 1e-3;

/// Avanza el valor mostrado `elapsed` segundos **reales** hacia `target` con una persecucion
/// exponencial: `rise_seconds` al subir y `fall_seconds` al bajar, hasta el 95 %. Es
/// independiente de como se trocee el tiempo (salvo el ajuste final de
/// `REVEAL_SNAP_EPSILON`) y no se limita como el `dt` de la fisica.
///
/// Politica defensiva, siempre con resultado finito en `[0, 1]`: `target` no finito se trata
/// como 0 (oculta) y se recorta a `[0, 1]`; `shown` no finito salta al objetivo y se recorta;
/// `elapsed` NaN, negativo o cero no avanza, y `+inf` llega al objetivo.
pub fn step_reveal(shown: f64, target: f64, elapsed: f64, settings: &RevealSettings) -> f64 {
    let target = if target.is_finite() {
        target.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !shown.is_finite() {
        return target;
    }
    let shown = shown.clamp(0.0, 1.0);
    if elapsed.is_nan() || elapsed <= 0.0 {
        return shown;
    }
    let seconds = if target > shown {
        settings.rise_seconds
    } else {
        settings.fall_seconds
    };
    let rate = 20.0_f64.ln() / seconds;
    let next = target + (shown - target) * (-rate * elapsed).exp();
    if (next - target).abs() <= REVEAL_SNAP_EPSILON {
        target
    } else {
        next.clamp(0.0, 1.0)
    }
}
