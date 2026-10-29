//! Sol con ojo (composicion del preview 03, aprobada visualmente por Charlie; opt-in): el Sol
//! con el ojo de la criatura delante y su sustitucion por la misma criatura. La usan el juego
//! (`--creature-solar-reveal`, `visual::solar_game`) y `examples/solar_reveal_preview.rs`; no
//! cambia las etapas, la mirada o el seguimiento aprobados.

use crate::{
    assets::obj::Model,
    math::{Transform, Vec3},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        pipeline::draw_model_with,
        shader::{Color, FragmentInput, Material, to_rgb8},
        stats::RenderStats,
    },
    scene::camera::Camera,
    visual::{
        creature_reveal::blend_rgb8,
        staged_creature_reveal::{StagedRevealSettings, staged_alphas},
    },
};

/// Progresos `(p_ojo, p_contorno)` de las etapas validadas: el ojo queda completo en
/// `eye_full_oxygen` y el cuerpo llega a `hint_alpha` en `hint_oxygen`. Misma formula que las
/// etapas aprobadas (ajustes ya validados: `0 < p_ojo < p_contorno < 1`).
pub fn solar_breakpoints(settings: &StagedRevealSettings) -> (f64, f64) {
    let span = settings.start_oxygen() - settings.full_oxygen();
    (
        (settings.start_oxygen() - settings.eye_full_oxygen()) / span,
        (settings.start_oxygen() - settings.hint_oxygen()) / span,
    )
}

/// Alfa del Sol visible, otro canal del **mismo progreso mostrado** de las etapas (no el O2
/// crudo ni un reloj propio): 1 hasta que el ojo queda completo (`p_ojo`), `1 - smoothstep`
/// hasta el contorno (`p_contorno`) y 0 despues. Con los ajustes por defecto: 1 con O2 >= 30 %,
/// fundido entre 30 % y 25 %, 0 con O2 <= 25 %. No es `1 - cuerpo` (al 25 % el cuerpo vale
/// 0.02 y el Sol ya 0). Monotona, continua y en `[0, 1]`; progreso NaN se trata como 0 (como
/// `staged_alphas`: ojo oculto, Sol normal) y fuera de `[0, 1]` se recorta.
pub fn solar_alpha(progress: f64, settings: &StagedRevealSettings) -> f64 {
    let p = if progress.is_nan() {
        0.0
    } else {
        progress.clamp(0.0, 1.0)
    };
    let (eye_point, hint_point) = solar_breakpoints(settings);
    if p <= eye_point {
        return 1.0;
    }
    if p >= hint_point {
        return 0.0;
    }
    let t = (p - eye_point) / (hint_point - eye_point);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// Los tres alfas del preview para un progreso mostrado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolarRevealAlphas {
    pub progress: f64,
    pub sun: f64,
    pub eye: f64,
    pub body: f64,
}

/// Luz con la que se sombrea **solo la criatura** del preview (los planetas, las orbitas y la
/// luz global del mundo no cambian). Con la criatura en el centro del Sol, la luz global
/// (`SolarSystem::light_position`, el origen) queda dentro de ella y el Lambert la deja en el
/// ambiente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolarRevealLight {
    /// Baseline: la luz global del mundo, sin cambios.
    World,
    /// Luz aprobada con la composicion del preview 03 (override local, etiquetado; la usa el
    /// juego solar): la luz en la posicion de la camara, un relleno frontal que se mueve con ella.
    Camera,
}

/// `solar_alpha` y `staged_alphas` (aprobadas, sin cambios) del mismo progreso.
pub fn solar_reveal_alphas(progress: f64, settings: &StagedRevealSettings) -> SolarRevealAlphas {
    let (eye, body) = staged_alphas(progress, settings);
    SolarRevealAlphas {
        progress,
        sun: solar_alpha(progress, settings),
        eye,
        body,
    }
}

/// Capa de la pareja central del preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairLayerKind {
    Sun,
    Body,
    Eye,
}

impl PairLayerKind {
    fn slot(self) -> usize {
        match self {
            Self::Sun => 0,
            Self::Body => 1,
            Self::Eye => 2,
        }
    }
    /// Desempate a igual profundidad (menor = delante): el ojo antes que el cuerpo, como la
    /// ruta de etapas (el cuerpo se dibuja despues y no gana con la misma profundidad).
    fn rank(self) -> u8 {
        match self {
            Self::Eye => 0,
            Self::Sun => 1,
            Self::Body => 2,
        }
    }
}

/// Lo que dibuja una capa: modelo, transform y material prestados (sin copias), opacidad
/// lineal y su shader de siempre.
pub struct PairLayer<'a> {
    pub kind: PairLayerKind,
    pub model: &'a Model,
    pub transform: &'a Transform,
    pub material: Material,
    pub alpha: f64,
    pub shade: &'a dyn Fn(&FragmentInput, &Material) -> Color,
}

/// Compositor de la pareja central (Sol + criatura), lo **ultimo** del 3D del cuadro.
///
/// 1. Cada capa (Sol, cuerpo, ojo) con alfa positiva se dibuja con el pipeline real en un
///    framebuffer de trabajo propio, asi que por pixel queda su superficie **mas cercana**;
///    solo se anota si gana el z-test contra el cuadro ya dibujado (estrellas, planetas y
///    nave: la profundidad del resto del mundo se respeta tal cual).
/// 2. Por pixel anotado, las capas se ordenan por una clave de profundidad con **una regla de
///    composicion**: el ojo usa `min(z_ojo, z_sol)` cuando el Sol esta anotado, asi que nunca
///    queda detras del Sol aunque su geometria este dentro de la esfera. Es una **ilusion de
///    composicion** respecto al Sol, no una colision 3D (con la escala aprobada el iris esta
///    muy por detras de la superficie solar). Consecuencia admitida: si el cuerpo esta entre
///    las dos (`z_sol < z_cuerpo < z_ojo`), la clave pone el ojo delante del cuerpo tambien; con
///    Sol y cuerpo a la vez NO se garantiza el orden fisico ojo/cuerpo. Sin Sol el orden es el
///    de las profundidades reales (desempate: ojo, Sol, cuerpo).
/// 3. Se mezclan de atras hacia delante sobre el fondo real con la mezcla lineal de las
///    etapas (`blend_rgb8`; alfa 1 copia el color) y se escribe una vez por pixel el color y
///    la profundidad real mas cercana de la pareja (`Framebuffer::write`).
///
/// Contrato de uso (el que cumple `World::render_solar_reveal_preview`; no es una API general):
/// - lo **ultimo** del 3D del cuadro: lo dibujado despues trataria la pareja como opaca;
/// - como mucho **una capa por tipo**: dos del mismo tipo no estan soportadas (mezclarian sus
///   anotaciones); de cada capa solo cuenta su superficie mas cercana (no es transparencia
///   general);
/// - una capa con alfa 0 o NaN no se dibuja ni deja color o profundidad.
///
/// Precondicion del framebuffer: construido con `Framebuffer::new` y sin cambiar despues sus
/// dimensiones ni el largo de sus pixeles. Antes de reservar o tocar nada se valida lo
/// observable (`width * height` sin desborde, en `1..=MAX_PIXELS` e igual a `pixels.len()`);
/// la profundidad del framebuffer es privada y sin getter, asi que su coherencia con esas
/// dimensiones NO se puede comprobar aqui: queda como precondicion.
///
/// Sin Sol, el resultado es el de la ruta de etapas (ojo opaco y cuerpo fantasma de una capa).
/// Los buffers se reutilizan y se ajustan si cambian las dimensiones. Los contadores son los
/// del dibujo de las capas en el framebuffer de trabajo (la escritura final no se cuenta);
/// no se afirma nada de rendimiento.
#[derive(Default)]
pub struct SolarPairCompositor {
    scratch: Option<Framebuffer>,
    colors: [Vec<u32>; 3],
    depths: [Vec<f64>; 3],
    marks: [Vec<bool>; 3],
    seen: Vec<bool>,
    touched: Vec<u32>,
}

impl SolarPairCompositor {
    pub fn new() -> Self {
        Self::default()
    }
    /// Memoria reservada por los buffers de trabajo, en bytes.
    pub fn buffer_bytes(&self) -> usize {
        let scratch = self.scratch.as_ref().map_or(0, |s| {
            s.pixels.capacity() * (size_of::<u32>() + size_of::<f64>())
        });
        scratch
            + (0..3)
                .map(|s| {
                    self.colors[s].capacity() * size_of::<u32>()
                        + self.depths[s].capacity() * size_of::<f64>()
                        + self.marks[s].capacity() * size_of::<bool>()
                })
                .sum::<usize>()
            + self.seen.capacity() * size_of::<bool>()
            + self.touched.capacity() * size_of::<u32>()
    }
    /// Dibuja y compone `layers` sobre el cuadro actual (ver el tipo).
    pub fn render(
        &mut self,
        framebuffer: &mut Framebuffer,
        camera: &Camera,
        layers: &[PairLayer<'_>],
    ) -> Result<RenderStats, String> {
        let (width, height) = (framebuffer.width, framebuffer.height);
        // Antes de reservar o tocar nada: dimensiones sin desborde, en `1..=MAX_PIXELS` y
        // coherentes con los pixeles publicos (ver la precondicion del tipo).
        let pixels = width
            .checked_mul(height)
            .filter(|&n| (1..=MAX_PIXELS).contains(&n) && n == framebuffer.pixels.len())
            .ok_or_else(|| {
                format!(
                    "framebuffer incoherente: {width}x{height} con {} pixeles (1..={MAX_PIXELS})",
                    framebuffer.pixels.len()
                )
            })?;
        if self.seen.len() != pixels {
            for slot in 0..3 {
                self.colors[slot].clear();
                self.colors[slot].resize(pixels, 0);
                self.depths[slot].clear();
                self.depths[slot].resize(pixels, f64::INFINITY);
                self.marks[slot].clear();
                self.marks[slot].resize(pixels, false);
            }
            self.seen.clear();
            self.seen.resize(pixels, false);
        }
        if self
            .scratch
            .as_ref()
            .is_none_or(|s| s.width != width || s.height != height)
        {
            self.scratch = Some(Framebuffer::new(width, height, 0)?);
        }
        let scratch = self.scratch.as_mut().expect("creado arriba");
        self.touched.clear();
        let mut alphas = [0.0; 3];
        let mut stats = RenderStats::default();
        for layer in layers {
            if layer.alpha.is_nan() || layer.alpha <= 0.0 {
                continue;
            }
            let slot = layer.kind.slot();
            alphas[slot] = layer.alpha.min(1.0);
            scratch.clear();
            let world: &Framebuffer = framebuffer;
            let (color, depth, mark) = (
                &mut self.colors[slot],
                &mut self.depths[slot],
                &mut self.marks[slot],
            );
            let (seen, touched) = (&mut self.seen, &mut self.touched);
            stats += draw_model_with(
                scratch,
                layer.model,
                layer.transform,
                &layer.material,
                camera,
                |fragment, material| {
                    // Centro del pixel (x + 0.5, y + 0.5): truncar da el pixel. El ultimo
                    // fragmento que llega a cada pixel es el mas cercano de la capa.
                    let p = fragment.screen_position;
                    let (x, y) = (p.x as usize, p.y as usize);
                    let index = y * width + x;
                    if !seen[index] {
                        seen[index] = true;
                        touched.push(index as u32);
                    }
                    let visible = world.depth_passes(x as isize, y as isize, p.z);
                    mark[index] = visible;
                    if !visible {
                        return Vec3::ZERO;
                    }
                    let shaded = (layer.shade)(fragment, material);
                    color[index] = to_rgb8(shaded);
                    depth[index] = p.z;
                    shaded
                },
            );
        }
        let sun = PairLayerKind::Sun.slot();
        for &index in &self.touched {
            let i = index as usize;
            self.seen[i] = false;
            // Marcas de este cuadro (se limpian aqui para el siguiente).
            let marked: [bool; 3] =
                std::array::from_fn(|slot| std::mem::take(&mut self.marks[slot][i]));
            // (clave de orden, desempate, alfa, color, profundidad real)
            let mut entries = [(0.0, 0, 0.0, 0, 0.0); 3];
            let mut count = 0;
            for kind in [PairLayerKind::Sun, PairLayerKind::Body, PairLayerKind::Eye] {
                let slot = kind.slot();
                if !marked[slot] || alphas[slot] <= 0.0 {
                    continue;
                }
                let real = self.depths[slot][i];
                // Regla de composicion: el ojo nunca queda detras del Sol.
                let key = if kind == PairLayerKind::Eye && marked[sun] && alphas[sun] > 0.0 {
                    real.min(self.depths[sun][i])
                } else {
                    real
                };
                entries[count] = (key, kind.rank(), alphas[slot], self.colors[slot][i], real);
                count += 1;
            }
            if count == 0 {
                continue;
            }
            let entries = &mut entries[..count];
            entries.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut color = framebuffer.pixels[i];
            let mut nearest = f64::INFINITY;
            for &(_, _, alpha, layer_color, real) in entries.iter().rev() {
                color = if alpha >= 1.0 {
                    layer_color
                } else {
                    blend_rgb8(color, layer_color, alpha)
                };
                nearest = nearest.min(real);
            }
            framebuffer.write((i % width) as isize, (i / width) as isize, nearest, color);
        }
        Ok(stats)
    }
}
