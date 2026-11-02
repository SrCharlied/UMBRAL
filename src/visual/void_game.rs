//! Ojo del vacio aprobado (UMB-309, variante 02) en el juego real, opt-in con `--void-eye`:
//! solo cambia el shader del planeta Vigilia existente (`LitPlanet` <-> `VoidEye`). Material,
//! radio, orbita, fase, rotacion, LOD y todo lo demas siguen siendo los de siempre. El ojo mira
//! siempre a la nave con el contexto vivo del mundo (`ship_position`, `elapsed_time`).

use crate::{
    renderer::shader::ShaderKind,
    scene::{solar_system::SolarSystem, world::World},
    visual::lucid_game::LucidSurfaceRequest,
};

/// Nombre del planeta existente que recibe el ojo del vacio.
pub const VOID_BODY_NAME: &str = "Vigilia";

/// Indice de Vigilia en `system`, validado sin tocar nada: un solo cuerpo con ese nombre, con el
/// shader de siempre (`LitPlanet`) o ya con el ojo (`VoidEye`), con radio y orbita finitos y
/// mayores que 0 (no es un sol) y con velocidades orbital/axial y fase finitas. Se busca por
/// nombre en cada llamada, sin guardar un indice que podria quedar obsoleto.
pub fn void_body_index(system: &SolarSystem) -> Result<usize, String> {
    let mut matches = system
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == VOID_BODY_NAME);
    let (index, body) = matches
        .next()
        .ok_or(format!("no hay planeta {VOID_BODY_NAME} en el sistema"))?;
    if matches.next().is_some() {
        return Err(format!(
            "planeta {VOID_BODY_NAME} ambiguo: hay mas de un cuerpo con ese nombre"
        ));
    }
    if !matches!(body.shader, ShaderKind::LitPlanet | ShaderKind::VoidEye) {
        return Err(format!(
            "{VOID_BODY_NAME} tiene el shader {:?}: solo se cambia LitPlanet <-> VoidEye",
            body.shader
        ));
    }
    if !(body.body_radius.is_finite() && body.body_radius > 0.0) {
        return Err(format!(
            "{VOID_BODY_NAME} necesita un radio finito mayor que 0: {}",
            body.body_radius
        ));
    }
    if !(body.orbit.radius.is_finite() && body.orbit.radius > 0.0) {
        return Err(format!(
            "{VOID_BODY_NAME} necesita una orbita finita mayor que 0 (no es un sol): {}",
            body.orbit.radius
        ));
    }
    let orbit = &body.orbit;
    if ![orbit.angular_speed, orbit.axial_speed, body.phase]
        .iter()
        .all(|x| x.is_finite())
    {
        return Err(format!(
            "{VOID_BODY_NAME} necesita datos de orbita finitos: velocidad {}, rotacion {}, fase {}",
            orbit.angular_speed, orbit.axial_speed, body.phase
        ));
    }
    Ok(index)
}

/// Pone (`enabled`) o quita el ojo del vacio de Vigilia y devuelve su indice. Idempotente;
/// quitarlo vuelve a `LitPlanet`. Si Vigilia no valida devuelve `Err` sin tocar el mundo.
pub fn set_void_eye(world: &mut World, enabled: bool) -> Result<usize, String> {
    let index = void_body_index(&world.system)?;
    world.system.bodies[index].shader = if enabled {
        ShaderKind::VoidEye
    } else {
        ShaderKind::LitPlanet
    };
    Ok(index)
}

/// Los flags de la linea de comandos que importan para `--void-eye` son los mismos que para
/// `--lucid-surface` (sin archivos).
pub type VoidEyeRequest = LucidSurfaceRequest;

/// Combinaciones de `--void-eye`, sin tocar archivos. Se valida antes de expandir los flags
/// implicitos del Sol con ojo. Solo dos rutas, cada una con o sin `--lucid-surface`: el juego
/// normal (con o sin la criatura estatica y su revelacion) y `--creature-solar-reveal`, donde las
/// demas combinaciones las decide su propio guard. Fuera del Sol no se apila con las rutas de
/// comparacion de la mirada: un error claro en vez de apagar una de las dos en silencio.
pub fn validate_void_eye_flags(void: bool, request: VoidEyeRequest) -> Result<(), String> {
    if !void {
        return Ok(());
    }
    if request.legacy {
        return Err("--legacy no admite --void-eye".into());
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
                "--void-eye solo se combina con el juego normal o con --creature-solar-reveal: no admite {flag}"
            ));
        }
    }
    Ok(())
}
