//! Estrellas observadoras aprobadas (UMB-310, preview02) en el juego real, opt-in con
//! `--watching-stars`.

use crate::{
    math::Mat4,
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        stats::RenderStats,
    },
    scene::world::World,
    visual::{
        lucid_game::LucidSurfaceRequest,
        solar_game::SolarGame,
        watching_stars::{WatchingStarsReport, render_watching_stars, sky_view_projection},
    },
};

/// Un cuadro del juego ya dibujado (antes de overlays y percepcion).
#[derive(Clone, Debug, PartialEq)]
pub struct GameFrame {
    /// Contadores del render 3D de la escena; los ojos (paso de color) no aportan nada.
    pub stats: RenderStats,
    /// Lo que dibujaron los ojos, o `None` sin `--watching-stars`.
    pub watching_stars: Option<WatchingStarsReport>,
}

/// Los ojos aprobados (preview02, `watching_stars`, sin cambios) sobre el cuadro YA dibujado:
/// la camara real del mundo, la posicion real de la nave y el O2 MOSTRADO
/// (`world.debug_oxygen01`, el mismo que ven entidades y percepcion). Solo reemplaza los pixeles;
/// la profundidad del cuadro no cambia.
pub fn apply_watching_stars(
    framebuffer: &mut Framebuffer,
    world: &World,
) -> Result<WatchingStarsReport, String> {
    let eyes = render_watching_stars(
        framebuffer,
        &world.camera,
        world.ship.position,
        world.debug_oxygen01,
    )?;
    framebuffer.pixels = eyes.pixels;
    Ok(eyes.report)
}

/// Preflight de los ojos ANTES de dibujar la escena: los mismos inputs observables, en el mismo
/// orden y con los mismos mensajes que la validacion (privada) del modulo congelado, para que un
/// `Err` no deje el cuadro del llamador limpiado o a medio dibujar.
fn preflight_watching_stars(framebuffer: &Framebuffer, world: &World) -> Result<(), String> {
    let oxygen01 = world.debug_oxygen01;
    if !(0.0..=1.0).contains(&oxygen01) {
        return Err(format!(
            "O2 de las estrellas observadoras fuera de [0, 1]: {oxygen01}"
        ));
    }
    if !world.ship.position.is_finite() {
        return Err("posicion de la nave no finita".into());
    }
    let camera = &world.camera;
    if !(camera.position.is_finite() && camera.target.is_finite() && camera.up.is_finite()) {
        return Err("camara con posicion, objetivo o up no finitos".into());
    }
    let forward = camera.target - camera.position;
    if !(forward.length() > 0.0 && camera.up.length() > 0.0) {
        return Err("camara sin direccion de vista o sin up".into());
    }
    let finite = |m: &Mat4| m.m.iter().flatten().all(|v| v.is_finite());
    if !(finite(&camera.view_projection()) && finite(&sky_view_projection(camera))) {
        return Err("camara con matrices no finitas".into());
    }
    let count = framebuffer
        .width
        .checked_mul(framebuffer.height)
        .filter(|&n| n > 0 && n <= MAX_PIXELS)
        .ok_or("dimensiones de cuadro invalidas o demasiado grandes")?;
    if framebuffer.pixels.len() != count {
        return Err(format!(
            "el cuadro tiene {} pixeles para {}x{}",
            framebuffer.pixels.len(),
            framebuffer.width,
            framebuffer.height
        ));
    }
    Ok(())
}

/// El cuadro del juego, igual en el cuadro inicial, headless, cada cuadro de la ventana y `P`: la
/// escena real (el Sol con ojo dibuja siempre con su `SolarGame::render`; si no, `World::render`)
/// y despues, con `watching_stars`, los ojos. Overlays, paneles y la percepcion van DESPUES, en el
/// bucle, asi los ojos nunca se pintan encima de un overlay y la percepcion sigue siendo el ultimo
/// paso de color.
pub fn render_game_frame(
    world: &World,
    framebuffer: &mut Framebuffer,
    solar: Option<&SolarGame>,
    watching_stars: bool,
) -> Result<GameFrame, String> {
    if watching_stars {
        preflight_watching_stars(framebuffer, world)?;
    }
    let stats = match solar {
        Some(game) => game.render(world, framebuffer)?,
        None => world.render(framebuffer),
    };
    let watching_stars = if watching_stars {
        Some(apply_watching_stars(framebuffer, world)?)
    } else {
        None
    };
    Ok(GameFrame {
        stats,
        watching_stars,
    })
}

/// Combinaciones de `--watching-stars` (que implica `--lucid-perception`), sin tocar archivos. Se
/// valida antes de expandir los flags implicitos del Sol con ojo. Las mismas dos escenas que la
/// percepcion: el juego normal (con o sin la criatura estatica y su revelacion) y
/// `--creature-solar-reveal`, donde las demas combinaciones las decide su propio guard; cada una
/// con o sin `--lucid-surface` y `--void-eye`. Fuera del Sol no se apila con las rutas de
/// comparacion de la mirada: un error claro en vez de apagar una de las dos en silencio.
pub fn validate_watching_stars_flags(
    watching_stars: bool,
    request: LucidSurfaceRequest,
) -> Result<(), String> {
    if !watching_stars {
        return Ok(());
    }
    if request.legacy {
        return Err("--legacy no admite --watching-stars".into());
    }
    if request.solar {
        return Ok(());
    }
    for (given, flag) in [
        (request.distant_turn, "--creature-distant-turn"),
        (request.horizontal_follow, "--creature-horizontal-follow"),
        (request.eye_compare, "--creature-eye-compare"),
    ] {
        if given {
            return Err(format!(
                "--watching-stars solo se combina con el juego normal o con --creature-solar-reveal: no admite {flag}"
            ));
        }
    }
    Ok(())
}
