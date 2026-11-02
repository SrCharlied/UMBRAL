//! Superficie lucida aprobada (UMB-308, variante 02) en el juego real, opt-in con
//! `--lucid-surface`: solo cambia el shader del planeta Antena existente. Material, radio,
//! orbita, rotacion, LOD y todo lo demas siguen siendo los de siempre.

use crate::{
    renderer::shader::ShaderKind,
    scene::{solar_system::SolarSystem, world::World},
};

/// Nombre del planeta existente que recibe la superficie lucida.
pub const LUCID_BODY_NAME: &str = "Antena";

/// Indice de Antena en `system`, validado sin tocar nada: un solo cuerpo con ese nombre, que no
/// es el Sol, con radio y orbita finitos y mayores que 0, y con el shader de siempre
/// (`LitPlanet`) o ya con la superficie lucida. Se busca por nombre en cada llamada, sin
/// guardar un indice que podria quedar obsoleto si cambian los cuerpos.
pub fn lucid_body_index(system: &SolarSystem) -> Result<usize, String> {
    let mut matches = system
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == LUCID_BODY_NAME);
    let (index, body) = matches
        .next()
        .ok_or(format!("no hay planeta {LUCID_BODY_NAME} en el sistema"))?;
    if matches.next().is_some() {
        return Err(format!(
            "planeta {LUCID_BODY_NAME} ambiguo: hay mas de un cuerpo con ese nombre"
        ));
    }
    if !matches!(
        body.shader,
        ShaderKind::LitPlanet | ShaderKind::LucidSurface
    ) {
        return Err(format!(
            "{LUCID_BODY_NAME} tiene el shader {:?}: solo se cambia LitPlanet <-> LucidSurface",
            body.shader
        ));
    }
    if !(body.body_radius.is_finite() && body.body_radius > 0.0) {
        return Err(format!(
            "{LUCID_BODY_NAME} necesita un radio finito mayor que 0: {}",
            body.body_radius
        ));
    }
    if !(body.orbit.radius.is_finite() && body.orbit.radius > 0.0) {
        return Err(format!(
            "{LUCID_BODY_NAME} necesita una orbita finita mayor que 0 (no es el Sol): {}",
            body.orbit.radius
        ));
    }
    Ok(index)
}

/// Pone (`enabled`) o quita la superficie lucida de Antena y devuelve su indice. Idempotente;
/// quitarla vuelve a `LitPlanet`. Si Antena no valida devuelve `Err` sin tocar el mundo.
pub fn set_lucid_surface(world: &mut World, enabled: bool) -> Result<usize, String> {
    let index = lucid_body_index(&world.system)?;
    world.system.bodies[index].shader = if enabled {
        ShaderKind::LucidSurface
    } else {
        ShaderKind::LitPlanet
    };
    Ok(index)
}

/// Lo que importa de la linea de comandos para `--lucid-surface` (sin archivos).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LucidSurfaceRequest {
    pub legacy: bool,
    /// `--creature-solar-reveal`.
    pub solar: bool,
    pub eye_compare: bool,
    pub distant_turn: bool,
    pub horizontal_follow: bool,
}

/// Combinaciones de `--lucid-surface`, sin tocar archivos. Se valida antes de expandir los flags
/// implicitos del Sol con ojo. Solo dos rutas: el juego normal (con o sin la criatura estatica y
/// su revelacion) y `--creature-solar-reveal`, donde las demas combinaciones las decide su propio
/// guard. Fuera del Sol no se apila con las rutas de comparacion de la mirada: un error claro en
/// vez de apagar una de las dos en silencio.
pub fn validate_lucid_surface_flags(
    lucid: bool,
    request: LucidSurfaceRequest,
) -> Result<(), String> {
    if !lucid {
        return Ok(());
    }
    if request.legacy {
        return Err("--legacy no admite --lucid-surface".into());
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
                "--lucid-surface solo se combina con el juego normal o con --creature-solar-reveal: no admite {flag}"
            ));
        }
    }
    Ok(())
}
