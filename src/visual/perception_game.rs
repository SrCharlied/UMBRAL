//! Percepcion alterada aprobada (UMB-310, variante 02) en el juego real, opt-in con
//! `--lucid-perception`. El modulo aprobado (`lucid_perception_variant`) se usa tal cual, como
//! ultimo paso de color sobre el cuadro ya dibujado.

use crate::{
    renderer::framebuffer::Framebuffer,
    scene::world::World,
    visual::{
        creature_reveal::step_debug_oxygen, lucid_game::LucidSurfaceRequest,
        lucid_perception_variant::lucid_perception_variant,
    },
};

/// Combinaciones de `--lucid-perception`, sin tocar archivos (los flags que importan son los de
/// `--lucid-surface`). Se valida antes de expandir los flags implicitos del Sol con ojo. Solo dos
/// escenas, cada una con o sin `--lucid-surface` y `--void-eye`: el juego normal (con o sin la
/// criatura estatica y su revelacion) y `--creature-solar-reveal`, donde las demas combinaciones
/// las decide su propio guard. Fuera del Sol no se apila con las rutas de comparacion de la
/// mirada: un error claro en vez de apagar una de las dos en silencio.
pub fn validate_lucid_perception_flags(
    perception: bool,
    request: LucidSurfaceRequest,
) -> Result<(), String> {
    if !perception {
        return Ok(());
    }
    if request.legacy {
        return Err("--legacy no admite --lucid-perception".into());
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
                "--lucid-perception solo se combina con el juego normal o con --creature-solar-reveal: no admite {flag}"
            ));
        }
    }
    Ok(())
}

/// Velocidad de la transicion del O2 mostrado hacia el objetivo manual, en unidades de O2 por
/// segundo REAL, igual al bajar y al recuperar (de 100 % a 0 en 4 s). Parametro tecnico
/// provisional de respuesta, no un ajuste artistico.
pub const OXYGEN_TRANSITION_RATE: f64 = 0.25;

/// Teclas de O2 de un cuadro, ya leidas: `preset` (1-4) y `M`/`N` cambian el OBJETIVO.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OxygenKeys {
    pub preset: Option<f64>,
    pub increase: bool,
    pub decrease: bool,
}

/// O2 manual en dos partes: el objetivo que fijan las teclas y el O2 mostrado, que lo persigue
/// en linea recta a `OXYGEN_TRANSITION_RATE` con segundos reales. Nada lo consume solo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OxygenTransition {
    target: f64,
    shown: f64,
}

impl OxygenTransition {
    /// Arranca con objetivo y mostrado EXACTAMENTE en `oxygen01` (sin presimular).
    pub fn new(oxygen01: f64) -> Result<Self, String> {
        let oxygen01 = valid_oxygen(oxygen01, "O2 inicial")?;
        Ok(Self {
            target: oxygen01,
            shown: oxygen01,
        })
    }
    pub fn target(&self) -> f64 {
        self.target
    }
    pub fn shown(&self) -> f64 {
        self.shown
    }
    /// Teclas del cuadro sobre el OBJETIVO, con los significados de siempre: el preset primero y
    /// despues `M`/`N` en pasos de 5 % (`step_debug_oxygen`). El O2 mostrado no cambia aqui. Un
    /// preset invalido devuelve `Err` sin tocar nada.
    pub fn press(&mut self, keys: OxygenKeys) -> Result<(), String> {
        let base = match keys.preset {
            Some(value) => valid_oxygen(value, "preset de O2")?,
            None => self.target,
        };
        self.target = step_debug_oxygen(base, keys.increase, keys.decrease);
        Ok(())
    }
    /// Avanza el O2 mostrado hacia el objetivo `OXYGEN_TRANSITION_RATE * real_seconds` en linea
    /// recta; si el resto cabe en el paso llega EXACTAMENTE (sin pasarse ni acercarse para
    /// siempre). `real_seconds` son segundos reales enteros (no el `dt` limitado de la fisica);
    /// `0` no cambia nada y un valor negativo o no finito devuelve `Err` sin tocar nada.
    pub fn advance(&mut self, real_seconds: f64) -> Result<f64, String> {
        if !(real_seconds.is_finite() && real_seconds >= 0.0) {
            return Err(format!(
                "segundos reales de la transicion de O2 invalidos (>= 0): {real_seconds}"
            ));
        }
        let step = OXYGEN_TRANSITION_RATE * real_seconds;
        let gap = self.target - self.shown;
        self.shown = if gap.abs() <= step {
            self.target
        } else {
            self.shown + step.copysign(gap)
        };
        Ok(self.shown)
    }
}

fn valid_oxygen(value: f64, name: &str) -> Result<f64, String> {
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{name} fuera de [0, 1]: {value}"))
    }
}

/// Foco del tunel: la nave proyectada con la camara real del cuadro, en `[0, 1]^2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipFocus {
    pub uv: [f64; 2],
    /// `false` si no hubo proyeccion valida (y `uv` es el centro de reserva).
    pub projected: bool,
}

/// Estado para el titulo de la ventana (sin HUD sobre la imagen): O2 mostrado y objetivo.
pub fn perception_title(oxygen: &OxygenTransition) -> String {
    format!(
        "PERCEPCION | O2 {:.0}% -> objetivo {:.0}% (1-4, M/N)",
        oxygen.shown() * 100.0,
        oxygen.target() * 100.0
    )
}

/// Foco de reserva cuando la nave no tiene proyeccion valida: el centro de la pantalla.
pub const FALLBACK_FOCUS_UV: [f64; 2] = [0.5, 0.5];

/// La nave (`world.ship.position`) proyectada con la camara real del mundo
/// (`camera.view_projection()`), en coordenadas de pantalla `[0, 1]^2` (y hacia abajo).
/// - Delante de la camara y en pantalla: su posicion proyectada exacta.
/// - Delante pero fuera de la pantalla: acotada al borde mas cercano (el tunel se abre desde ahi).
/// - Detras de la camara (`w <= 0`) o con valores no finitos: el centro, con `projected = false`.
///
/// Con la camara de persecucion del juego la nave siempre queda delante y en pantalla.
pub fn ship_focus(world: &World) -> ShipFocus {
    let clip = world
        .camera
        .view_projection()
        .transform(world.ship.position.extend(1.0));
    let uv = [clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5];
    if clip.w > 0.0 && uv.iter().all(|v| v.is_finite()) {
        ShipFocus {
            uv: uv.map(|v| v.clamp(0.0, 1.0)),
            projected: true,
        }
    } else {
        ShipFocus {
            uv: FALLBACK_FOCUS_UV,
            projected: false,
        }
    }
}

/// Ultimo paso de color del cuadro: la variante 02 aprobada con el O2 del mundo, su reloj y la
/// nave proyectada como foco. Solo reemplaza los pixeles (no la profundidad).
pub fn apply_lucid_perception(framebuffer: &mut Framebuffer, world: &World) -> Result<(), String> {
    framebuffer.pixels = lucid_perception_variant(
        framebuffer,
        world.debug_oxygen01,
        world.time,
        ship_focus(world).uv,
        false,
    )?;
    Ok(())
}
