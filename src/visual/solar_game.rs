//! Sol con ojo en el juego real (`--creature-solar-reveal`, opt-in): la composicion del preview
//! 03 **aprobada por Charlie** llevada al bucle de la ventana sin reimplementarla. Este modulo
//! solo envuelve las APIs ya aceptadas de `World` (etapas, mirada, seguimiento horizontal 02 y
//! `render_solar_reveal_preview`): arranque, vistas, teclas y titulo. Sin el flag, el juego no
//! pasa por aqui. Ver `docs/static-creature.md`.

use crate::{
    renderer::{framebuffer::Framebuffer, stats::RenderStats},
    scene::world::{SolarBodyOverride, World},
    visual::{
        creature_eye_comparison::EyeTarget,
        creature_eye_follow::EyeFollowSettings,
        solar_reveal_preview::SolarRevealLight,
        staged_creature_reveal::{CREATURE_EYE_REGION, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

/// Radio en mundo y giro de la criatura en el centro del Sol (los del preview 03 aprobado).
pub const SOLAR_CREATURE_RADIUS: f64 = 80.0;
pub const SOLAR_CREATURE_YAW: f64 = 0.0;
/// Luz local de la criatura aprobada en el preview 03 (relleno frontal de camara).
const SOLAR_LIGHT: SolarRevealLight = SolarRevealLight::Camera;
const DEG: f64 = std::f64::consts::PI / 180.0;

/// Vistas del juego solar: los dos perfiles aprobados del preview 03.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolarView {
    /// Arranque (`7` y `R`): nave a 298 del centro del Sol en +Z; camara a ~320.
    Frontal,
    /// `8`: nave a 70 grados hacia +X, 578 de distancia y 60 de altura; camara a ~600.
    Oblique,
}

impl SolarView {
    /// `(azimut, distancia, altura)` de `World::place_solar_reveal_camera`, con las mismas
    /// expresiones que `examples/solar_reveal_preview.rs`.
    pub fn profile(self) -> (f64, f64, f64) {
        match self {
            Self::Frontal => (0.0, 298.0, 0.0),
            Self::Oblique => (70.0 * DEG, 578.0, 60.0),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Frontal => "FRONTAL",
            Self::Oblique => "OBLICUA",
        }
    }
}

/// Lo demas que pide la linea de comandos (o `GameOptions`) junto al flag solar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SolarRevealRequest {
    pub has_creature: bool,
    pub legacy: bool,
    /// `--near N`.
    pub near_body: bool,
    pub near_creature: bool,
    /// `--creature-reveal` (V1).
    pub uniform_reveal: bool,
    pub distant_turn: bool,
}

/// Combinaciones de `--creature-solar-reveal`, sin tocar archivos. Se valida justo despues de
/// `CreatureOptions::from_flags` y antes de los demas validadores: el flag implica las etapas,
/// la mirada, la comparacion y el seguimiento horizontal (redundantes si se pasan), y rechaza
/// las rutas que cambiarian la composicion aprobada o el arranque.
pub fn validate_solar_reveal_flags(solar: bool, request: SolarRevealRequest) -> Result<(), String> {
    if !solar {
        return Ok(());
    }
    if request.legacy {
        return Err("--legacy no admite --creature-solar-reveal".into());
    }
    if !request.has_creature {
        return Err(
            "--creature-solar-reveal necesita --creature-model y --creature-texture".into(),
        );
    }
    let conflict = |flag: &str, why: &str| {
        Err(format!(
            "--creature-solar-reveal y {flag} son incompatibles: {why}"
        ))
    };
    if request.uniform_reveal {
        return conflict("--creature-reveal", "el Sol con ojo usa las etapas");
    }
    if request.near_body {
        return conflict("--near", "el arranque es la vista FRONTAL del Sol");
    }
    if request.near_creature {
        return conflict("--near-creature", "el arranque es la vista FRONTAL del Sol");
    }
    if request.distant_turn {
        return conflict(
            "--creature-distant-turn",
            "el Sol con ojo usa el seguimiento horizontal aprobado",
        );
    }
    Ok(())
}

/// Colocacion de la criatura en el centro del Sol (el cuerpo sin orbita) con el radio y el giro
/// aprobados, igual que el preview. `enable_solar_reveal_preview` revalida despues el Sol.
pub fn solar_creature_placement(world: &World) -> Result<CreaturePlacement, String> {
    let sun = world
        .system
        .bodies
        .iter()
        .find(|b| b.orbit.radius == 0.0)
        .ok_or("el Sol con ojo necesita un Sol (cuerpo sin orbita)")?;
    CreaturePlacement::new(
        sun.position(world.time),
        SOLAR_CREATURE_RADIUS,
        SOLAR_CREATURE_YAW,
    )
}

/// Pulsaciones de un cuadro (sin repeticion). La ventana las lee; las pruebas las construyen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SolarKeys {
    /// `R`: vuelve a la vista FRONTAL del arranque.
    pub reset: bool,
    /// `7`: vista FRONTAL.
    pub frontal: bool,
    /// `8`: vista OBLICUA.
    pub oblique: bool,
    /// `H`: seguimiento horizontal ON/OFF (OFF vuelve al frente con su dinamica).
    pub follow: bool,
    /// `G`: el ojo mira a la CAMARA o a la NAVE.
    pub target: bool,
}

/// Estado del modo solar en la ventana: solo la vista elegida. Todo lo demas vive en `World`.
#[derive(Debug)]
pub struct SolarGame {
    view: SolarView,
}

impl SolarGame {
    /// Arranque del juego solar con la receta del preview 03 y las APIs aceptadas, en su orden:
    /// la criatura (una carga, ya colocada con `solar_creature_placement`), las etapas (particion
    /// una vez), la mirada a la CAMARA (calibracion una vez), el seguimiento horizontal 02, el
    /// preview con la luz de camara y la vista FRONTAL. Usa el O2 de depuracion ya puesto en
    /// `world`. No congela las orbitas ni simula tiempo: en la vista frontal el cuadro de
    /// arranque ya es el asentado del preview. `Err` si algo no valida.
    pub fn start(world: &mut World, creature: StaticCreature) -> Result<Self, String> {
        world.set_creature(creature)?;
        world
            .enable_staged_creature_reveal(StagedRevealSettings::default(), CREATURE_EYE_REGION)?;
        world.enable_creature_eye_follow_targeting(
            EyeFollowSettings::default(),
            EyeTarget::Camera,
        )?;
        world.enable_creature_horizontal_follow()?;
        world.enable_solar_reveal_preview(SOLAR_LIGHT)?;
        let mut game = Self {
            view: SolarView::Frontal,
        };
        game.place(world, SolarView::Frontal)?;
        Ok(game)
    }
    pub fn view(&self) -> SolarView {
        self.view
    }
    /// Cuadro del juego solar: **siempre** `World::render_solar_reveal_preview` (el inicial, el
    /// headless y cada cuadro del bucle); con el ojo oculto es exactamente `World::render`.
    pub fn render(&self, world: &World, fb: &mut Framebuffer) -> Result<RenderStats, String> {
        world.render_solar_reveal_preview(fb)
    }
    /// `render` con una capa `Body` alternativa opcional (debug opt-in de la composicion del
    /// tentaculo; `None` es exactamente `render`). Ver `World::render_solar_reveal_preview_with`.
    pub fn render_with(
        &self,
        world: &World,
        fb: &mut Framebuffer,
        body: Option<&SolarBodyOverride<'_>>,
    ) -> Result<RenderStats, String> {
        world.render_solar_reveal_preview_with(fb, body)
    }
    /// Teclas del cuadro en orden `R`, `7`, `8`, `H`, `G`. Las vistas usan la pose del preview,
    /// que salta la revelacion y la mirada al objetivo; el seguimiento conserva su giro y lo
    /// sigue con segundos reales (sin salto nuevo). Conservan el O2, el reloj orbital, `G` y `H`.
    pub fn apply(&mut self, world: &mut World, keys: SolarKeys) -> Result<(), String> {
        if keys.reset || keys.frontal {
            self.place(world, SolarView::Frontal)?;
        }
        if keys.oblique {
            self.place(world, SolarView::Oblique)?;
        }
        if keys.follow {
            let active = world
                .creature_horizontal_follow()
                .is_some_and(|f| f.is_active());
            world.set_creature_horizontal_follow_active(!active)?;
        }
        if keys.target {
            let next = match world.creature_eye_target() {
                Some(EyeTarget::Camera) => EyeTarget::Ship,
                _ => EyeTarget::Camera,
            };
            world.set_creature_eye_target(next)?;
        }
        Ok(())
    }
    fn place(&mut self, world: &mut World, view: SolarView) -> Result<(), String> {
        let (azimuth, distance, height) = view.profile();
        world.place_solar_reveal_camera(azimuth, distance, height)?;
        self.view = view;
        Ok(())
    }
    /// Estado para el titulo de la ventana (sin HUD sobre la composicion).
    pub fn title(&self, world: &World) -> String {
        let target = match world.creature_eye_target() {
            Some(EyeTarget::Ship) => "NAVE",
            _ => "CAMARA",
        };
        let follow = world
            .creature_horizontal_follow()
            .is_some_and(|f| f.is_active());
        format!(
            "SOL CON OJO | O2 {:.0}% (1-4, M/N) | G {target} | H {} | {} (7/8, R)",
            world.debug_oxygen01 * 100.0,
            if follow { "ON" } else { "OFF" },
            self.view.name()
        )
    }
}
