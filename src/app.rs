//! Aplicacion del juego: bucle de ventana, entrada y modo headless.

use crate::{
    assets::{obj::load_model, ship_model_path},
    debug::overlay::{OverlayState, draw_overlay, overlay_lines},
    renderer::{
        framebuffer::Framebuffer,
        image::{save_png, save_png_exclusive},
        stats::RenderStats,
    },
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, SolarBodyOverride, World},
    },
    visual::{
        creature_distant_turn::{
            CreaturePreset, DISTANT_TURN_FAR_RADII, DISTANT_TURN_MAX_PITCH, DISTANT_TURN_MAX_YAW,
            DISTANT_TURN_NEAR_RADII, DISTANT_TURN_T95_SECONDS, FAR_PRESET_LATERAL,
            FAR_PRESET_RADII, distant_turn_lines, draw_distant_turn_panel,
        },
        creature_eye_comparison::{
            EyeTarget, draw_eye_comparison_panel, eye_comparison_lines, eye_comparison_panel,
            validate_eye_compare_flags,
        },
        creature_eye_follow::{EyeFollowSettings, GazeAngles, validate_eye_follow_flags},
        creature_horizontal_follow::{
            HORIZONTAL_FOLLOW_MAX_SPEED, HORIZONTAL_FOLLOW_T95_SECONDS, horizontal_follow_lines,
        },
        creature_reveal::{RevealSettings, step_debug_oxygen, validate_reveal_flags},
        creature_viewer::validate_output_path,
        ending::{
            EndingClock, EndingKind, EndingSummary, FULLY_SHOWN as ENDING_FULLY_SHOWN, draw_ending,
        },
        lucid_game::set_lucid_surface,
        monster_death::MonsterDeath,
        oxygen_hud::{OxygenHudOptions, OxygenReading, draw_oxygen_hud, validate_oxygen_hud_flags},
        perception_game::{
            OXYGEN_TRANSITION_RATE, OxygenKeys, OxygenTransition, apply_lucid_perception,
            perception_title, ship_focus,
        },
        solar_game::{
            SolarGame, SolarKeys, SolarRevealRequest, solar_creature_placement,
            validate_solar_reveal_flags,
        },
        staged_creature_reveal::{
            CREATURE_EYE_REGION, StagedRevealSettings, validate_staged_flags,
        },
        static_creature::{CreatureOptions, CreaturePlacement, StaticCreature},
        tentacle_auto_capture::{
            AutoCapture, AutoControls, AutoEvent, AutoFrame, advance_auto_capture_frame,
        },
        tentacle_grasp_closure::ClosureClip,
        tentacle_grasp_preview::{GraspPreview, PreviewInputs, arm_rest, load_preview_inputs},
        tentacle_live_pursuit::{LivePursuit, LivePursuitRequest},
        tentacle_solar_game::{CaptureToggle, SolarCaptureDebug, TentacleCaptureRequest},
        tentacle_spatial_pursuit::SpatialPursuit,
        void_game::set_void_eye,
        watching_stars_game::render_game_frame_with,
    },
};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{path::PathBuf, time::Instant};

pub struct GameOptions {
    pub width: usize,
    pub height: usize,
    pub headless: bool,
    pub frame_limit: Option<usize>,
    pub output: PathBuf,
    /// Tiempo simulado inicial en segundos (`--time`).
    pub time: f64,
    /// Indice de cuerpo junto al cual arrancar (`--near`).
    pub near: Option<usize>,
    pub culling: bool,
    pub lod: bool,
    pub overlay: bool,
    /// Criatura estatica opcional (debug, opt-in); `None` deja el juego como siempre.
    pub creature: Option<CreatureOptions>,
    /// Revelacion de la criatura por oxigeno (`--creature-reveal`, requiere la criatura).
    pub creature_reveal: bool,
    /// Revelacion por etapas (`--creature-reveal-stages`, requiere la criatura; excluyente
    /// con `creature_reveal`).
    pub creature_reveal_stages: bool,
    /// Mirada local hacia la nave (`--creature-eye-follow`, requiere `creature_reveal_stages`).
    pub creature_eye_follow: bool,
    /// Oxigeno de depuracion inicial en `[0, 1]` (`--debug-oxygen`, por defecto 1).
    pub debug_oxygen: f64,
}

/// Additive runtime configuration. Existing GameOptions callers retain the default scene.
#[derive(Clone, Copy, Debug, Default)]
pub enum OxygenSource {
    #[default]
    Legacy,
    Real,
    Debug,
}
pub struct ConfiguredGameOptions {
    pub options: GameOptions,
    pub gameplay_feedback: bool,
    pub oxygen_source: OxygenSource,
    /// Segundos que dura el tanque completo de oxigeno real (`--oxygen-seconds`); `None` deja
    /// el valor de `GameplaySettings` (420 s).
    pub oxygen_seconds: Option<f64>,
    /// Experiencia de presentacion (por defecto en ventana): espacio procedural por sectores con
    /// tanques visibles y marcadores, y teclas de O2 que arrastran el O2 real.
    pub presentation: bool,
    /// HUD de RESERVA de oxigeno: encendido con la experiencia completa (ventana) o con `--oxygen-hud`.
    pub oxygen_hud: OxygenHudOptions,
}

/// Ritmos de consumo que alterna la tecla `L`: segundos para vaciar el tanque completo.
/// 420 s es el juego normal; los demas sirven para demostrar muerte, percepcion y ojos.
pub const OXYGEN_PACE_SECONDS: [f64; 4] = [420.0, 120.0, 60.0, 30.0];

/// O2 que da cada tanque en la experiencia de presentacion (el juego clasico da 0.15).
pub const PRESENTATION_TANK_RECOVERY: f64 = 0.25;

/// Segundos que dura el tanque completo con el consumo actual (`inf` si no se consume).
pub fn oxygen_tank_seconds(world: &World) -> f64 {
    1.0 / world.gameplay.settings.drain_per_second
}

/// Fija la duracion del tanque completo (segundos finitos > 0). Conserva el oxigeno actual:
/// solo cambia la velocidad a partir de ahora. `Shift+R` conserva este ajuste.
pub fn set_oxygen_tank_seconds(world: &mut World, seconds: f64) -> Result<(), String> {
    if !(seconds.is_finite() && seconds > 0.0) {
        return Err(format!(
            "duracion del oxigeno invalida (segundos finitos > 0): {seconds}"
        ));
    }
    world.gameplay.settings.drain_per_second = 1.0 / seconds;
    Ok(())
}

/// Tecla `L`: pasa al siguiente ritmo mas rapido de `OXYGEN_PACE_SECONDS` y, tras el mas
/// rapido (o desde un valor fuera de la lista), vuelve al normal. Devuelve los segundos fijados.
pub fn cycle_oxygen_pace(world: &mut World) -> f64 {
    let current = oxygen_tank_seconds(world);
    let next = OXYGEN_PACE_SECONDS
        .iter()
        .position(|&s| (s - current).abs() < 1e-6)
        .and_then(|i| OXYGEN_PACE_SECONDS.get(i + 1))
        .copied()
        .unwrap_or(OXYGEN_PACE_SECONDS[0]);
    world.gameplay.settings.drain_per_second = 1.0 / next;
    next
}
impl std::ops::Deref for ConfiguredGameOptions {
    type Target = GameOptions;
    fn deref(&self) -> &GameOptions {
        &self.options
    }
}
impl From<GameOptions> for ConfiguredGameOptions {
    fn from(options: GameOptions) -> Self {
        Self {
            options,
            gameplay_feedback: false,
            oxygen_source: OxygenSource::Legacy,
            oxygen_seconds: None,
            presentation: false,
            oxygen_hud: OxygenHudOptions::default(),
        }
    }
}

/// Acciones de Persona 2, separadas del teclado para probarlas sin ventana.
#[derive(Clone, Copy, Debug)]
pub enum GameplayAction {
    ToggleOrbitAssist,
    Restart,
    ForceOxygen(f64),
    RealOxygen,
}

pub fn apply_gameplay_action(world: &mut World, action: GameplayAction) {
    match action {
        GameplayAction::ToggleOrbitAssist => world.orbit_assist ^= true,
        GameplayAction::Restart => world.restart_game(),
        GameplayAction::ForceOxygen(value) => world.set_debug_oxygen(value),
        GameplayAction::RealOxygen => world.clear_debug_oxygen(),
    }
}

/// Teclas de juego de UN cuadro, ya leidas del frontend (ventana, replay o pruebas). `full_restart` es
/// Shift+R y `reset` es R sin Shift. Las vistas de depuracion (F1, F3-F6), P y Esc no pasan por aqui.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameKeys {
    pub full_restart: bool,
    pub reset: bool,
    /// 7 (preset CERCA / vista FRONTAL).
    pub frontal: bool,
    /// 8 (preset LEJOS / vista OBLICUA).
    pub oblique: bool,
    /// H.
    pub follow: bool,
    /// G.
    pub target: bool,
    /// O (asistencia orbital).
    pub orbit_assist: bool,
    /// 0 (O2 real).
    pub real_oxygen: bool,
    /// 1-4 y M/N.
    pub oxygen: OxygenKeys,
    /// F7 (composicion observadora).
    pub capture_toggle: bool,
    /// X de diagnostico.
    pub emergency: bool,
}

impl FrameKeys {
    /// Estado terminal (sin O2 real o victoria): nave, captura y relojes de efecto quedan congelados
    /// hasta el reinicio completo, asi que solo Shift+R sigue disponible (USER_DECISIONS). Fuera de
    /// terminal, las teclas no cambian.
    pub fn gated(self, world: &World) -> Self {
        if world.gameplay.game_over || world.gameplay.objective_complete {
            Self {
                full_restart: self.full_restart,
                ..Self::default()
            }
        } else {
            self
        }
    }
}

/// Rutas que cambian el despacho de las teclas del bucle principal.
#[derive(Clone, Copy, Debug, Default)]
pub struct DispatchRoute {
    pub comparison: bool,
    pub distant_turn: bool,
    pub horizontal: bool,
    pub near_creature: bool,
    /// `--near <cuerpo>`.
    pub near_body: Option<usize>,
    /// Captura automatica: R/7/8/H/G se aplican en su cuadro compartido (filtradas si capturada).
    pub auto: bool,
}

/// Despacho de las teclas de juego de UN cuadro, antes de la fisica (el mismo para la ventana y las
/// pruebas). Devuelve las teclas solares que la captura automatica aplica en su cuadro compartido.
pub fn dispatch_frame_keys(
    world: &mut World,
    mut solar_game: Option<&mut SolarGame>,
    mut oxygen: Option<&mut OxygenTransition>,
    route: DispatchRoute,
    last_preset: &mut CreaturePreset,
    keys: FrameKeys,
) -> Result<SolarKeys, String> {
    let keys = keys.gated(world);
    let mut auto_solar = SolarKeys::default();
    if keys.full_restart {
        apply_gameplay_action(world, GameplayAction::Restart);
        if let Some(oxygen) = oxygen.as_deref_mut() {
            *oxygen = OxygenTransition::new(world.gameplay.oxygen01)?;
        }
        // Nueva partida = el arranque de la ruta (mismas colocaciones y preset que `run`), no solo la
        // nave de inicio: --near, --near-creature y el preset LEJOS de las candidatas.
        if let Some(index) = route.near_body {
            world.place_near(index, 25.0)?;
        }
        if route.near_creature {
            world.place_near_creature()?;
        }
        if route.distant_turn || route.horizontal {
            world.place_creature_preset(CreaturePreset::Far)?;
        }
        *last_preset = CreaturePreset::Far;
        if !route.auto
            && let Some(game) = solar_game.as_deref_mut()
        {
            game.apply(
                world,
                SolarKeys {
                    reset: true,
                    ..SolarKeys::default()
                },
            )?;
        }
    } else if keys.reset {
        if route.auto {
            auto_solar.reset = true;
        } else if let Some(game) = solar_game.as_deref_mut() {
            // Sol con ojo: R vuelve a la vista FRONTAL (conserva O2, G, H y el reloj).
            let reset = SolarKeys {
                reset: true,
                ..SolarKeys::default()
            };
            game.apply(world, reset)?;
        } else if route.distant_turn || route.horizontal {
            // Candidatas: R repite el ultimo preset (conserva O2, objetivo y H).
            world.place_creature_preset(*last_preset)?;
        } else if route.near_creature {
            // Con --near-creature, R vuelve al arranque frente a la criatura.
            world.place_near_creature()?;
        } else {
            world.reset_ship();
        }
    }
    if keys.orbit_assist {
        apply_gameplay_action(world, GameplayAction::ToggleOrbitAssist);
    }
    if keys.real_oxygen {
        apply_gameplay_action(world, GameplayAction::RealOxygen);
    }
    if oxygen.is_none() {
        // Sin --lucid-perception: el O2 salta como siempre.
        if let Some(value) = keys.oxygen.preset {
            world.debug_oxygen01 = value;
        }
        world.debug_oxygen01 = step_debug_oxygen(
            world.debug_oxygen01,
            keys.oxygen.increase,
            keys.oxygen.decrease,
        );
    }
    if route.distant_turn || route.horizontal {
        for (pressed, preset) in [
            (keys.frontal, CreaturePreset::Near),
            (keys.oblique, CreaturePreset::Far),
        ] {
            if pressed {
                *last_preset = preset;
                world.place_creature_preset(preset)?;
            }
        }
        if keys.follow {
            // Solo cambia la eleccion: apagado vuelve al reposo/frente con su transicion.
            if route.horizontal {
                let active = world
                    .creature_horizontal_follow()
                    .is_some_and(|f| f.is_active());
                world.set_creature_horizontal_follow_active(!active)?;
            } else {
                let active = world.creature_distant_turn().is_some_and(|t| t.is_active());
                world.set_creature_distant_turn_active(!active)?;
            }
        }
    }
    if let Some(game) = solar_game {
        let solar = SolarKeys {
            frontal: keys.frontal,
            oblique: keys.oblique,
            follow: keys.follow,
            target: keys.target,
            reset: false,
        };
        if route.auto {
            // La captura automatica aplica estas teclas en su cuadro compartido (sin R/7/8 si atrapada).
            auto_solar = SolarKeys {
                reset: auto_solar.reset,
                ..solar
            };
        } else {
            game.apply(world, solar)?;
        }
    }
    if route.comparison && keys.target {
        // Solo cambia la eleccion: la mirada llega con su transicion de siempre.
        let next = match world.creature_eye_target() {
            Some(EyeTarget::Camera) => EyeTarget::Ship,
            _ => EyeTarget::Camera,
        };
        world.set_creature_eye_target(next)?;
    }
    Ok(auto_solar)
}

/// Feedback minimo de recoleccion/estados usando la fuente existente del equipo.
/// No sustituye el HUD artistico de Persona 3.
pub fn gameplay_feedback_lines(world: &World) -> Vec<String> {
    let g = &world.gameplay;
    let mut lines = vec![format!(
        "OXIGENO {:.0}%  RECURSOS {}  REGISTROS {}/3",
        g.oxygen01 * 100.0,
        g.collected_resources,
        g.collected_logs
    )];
    lines.push(if g.game_over {
        "SIN OXIGENO - SHIFT+R: REINICIAR".into()
    } else if g.objective_complete {
        "OBJETIVO COMPLETO - SHIFT+R: REINICIAR".into()
    } else if g.collected_logs >= 3 {
        "OBJETIVO: ACERCATE AL SOL".into()
    } else {
        "OBJETIVO: REUNE LOS TRES REGISTROS DORADOS".into()
    });
    lines.push(format!(
        "O: ASISTENCIA {}  SHIFT+R: REINICIAR",
        if world.orbit_assist { "ON" } else { "OFF" }
    ));
    let target = g
        .logs
        .iter()
        .filter(|log| !log.anchor.collected)
        .filter_map(|log| {
            log.anchor
                .position(&world.system, world.time)
                .map(|p| (log, p))
        })
        .min_by(|(_, a), (_, b)| {
            (*a - world.ship.position)
                .length()
                .total_cmp(&(*b - world.ship.position).length())
        });
    if let Some((log, position)) = target {
        let delta = position - world.ship.position;
        let direction = delta.normalized();
        let horizontal = direction.dot(world.ship.right());
        let vertical = direction.dot(world.ship.up());
        let forward = direction.dot(world.ship.forward());
        lines.push(format!(
            "DESTINO REGISTRO {}: {:.0} U - {} {} {}",
            log.anchor.id + 1,
            delta.length(),
            if forward < 0.0 { "ATRAS" } else { "FRENTE" },
            if horizontal > 0.15 {
                "DERECHA"
            } else if horizontal < -0.15 {
                "IZQUIERDA"
            } else {
                "CENTRO"
            },
            if vertical > 0.15 {
                "ARRIBA"
            } else if vertical < -0.15 {
                "ABAJO"
            } else {
                "NIVEL"
            }
        ));
    }
    if world.forced_oxygen.is_some() {
        lines.push(format!(
            "VISUAL DEBUG {:.0}% - 0: OXIGENO REAL",
            world.oxygen01() * 100.0
        ));
    }
    if let Some(log) = g
        .last_log
        .and_then(|id| g.logs.iter().find(|log| log.anchor.id == id))
    {
        lines.push(format!("{}: {}", log.title, log.text));
    }
    lines
}

pub fn draw_gameplay_feedback(fb: &mut Framebuffer, world: &World) {
    let scale = if fb.width >= 960 && fb.height >= 540 {
        2
    } else {
        1
    };
    let columns = fb.width.saturating_sub(16) / (6 * scale);
    if columns == 0 {
        return;
    }
    let mut lines = Vec::new();
    for text in gameplay_feedback_lines(world) {
        let mut line = String::new();
        for word in text.split_whitespace() {
            if !line.is_empty() && line.len() + word.len() + 1 > columns {
                lines.push(line);
                line = String::new();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    let row_height = 8 * scale + 2;
    lines.truncate((fb.height / 3 / row_height).max(1));
    let y = fb.height.saturating_sub(lines.len() * row_height + 8);
    for py in y..fb.height {
        for x in 0..fb.width {
            fb.put_pixel(x, py, 0x080e18);
        }
    }
    for (i, line) in lines.iter().enumerate() {
        crate::debug::overlay::draw_text(fb, 8, y + 4 + i * row_height, scale, 0xdce8ed, line);
    }
}

/// Capas de presentacion sobre la escena ya dibujada.
#[derive(Clone, Copy, Debug)]
pub struct PresentationLayers<'a> {
    /// Lineas del overlay (F1) si esta visible.
    pub overlay: Option<&'a [String]>,
    pub comparison: bool,
    pub distant_turn: bool,
    pub horizontal: bool,
    /// Ultimo preset de las candidatas (sus paneles).
    pub preset: CreaturePreset,
    /// Franja tecnica de Persona 2: OPCIONAL (`--gameplay-feedback`), nunca por defecto.
    pub gameplay_feedback: bool,
    /// Presentacion final: segundos reales desde victoria o muerte (`EndingClock`); `None`
    /// mientras la partida sigue. Es la ultima capa: va sobre todo lo demas.
    pub ending: Option<f64>,
    /// Percepcion lucida: ultimo paso de color (con O2 0 no queda texto brillante).
    pub perception: bool,
}

/// Compositor UNICO de la presentacion para el cuadro headless, la ventana y `P`: overlay, paneles de
/// las candidatas, franja tecnica opcional y, al final, la percepcion. El mismo orden en todas las rutas.
pub fn compose_presentation(
    framebuffer: &mut Framebuffer,
    world: &World,
    layers: &PresentationLayers<'_>,
) -> Result<(), String> {
    compose_presentation_with_hud(framebuffer, world, layers, false)
}

/// `compose_presentation` con el HUD de RESERVA de oxigeno (encendido por defecto en la experiencia completa,
/// `--oxygen-hud` en las demas): el instrumento va DESPUES de la percepcion para seguir legible con O2 bajo
/// (decision de Charlie), debajo de los marcadores de navegacion y de la presentacion final. La reserva
/// (`world.gameplay.oxygen01`) se valida ANTES de tocar el cuadro: con un valor invalido nada se dibuja.
/// Con `oxygen_hud = false` es exactamente `compose_presentation`.
pub fn compose_presentation_with_hud(
    framebuffer: &mut Framebuffer,
    world: &World,
    layers: &PresentationLayers<'_>,
    oxygen_hud: bool,
) -> Result<(), String> {
    let reading = oxygen_hud
        .then(|| OxygenReading::real_reserve(world))
        .transpose()?;
    if let Some(lines) = layers.overlay {
        draw_overlay(framebuffer, lines);
    }
    if layers.comparison {
        draw_comparison(framebuffer, world);
    }
    if layers.distant_turn {
        draw_distant_turn(framebuffer, world, layers.preset);
    }
    if layers.horizontal {
        draw_horizontal_follow(framebuffer, world, layers.preset);
    }
    if layers.gameplay_feedback {
        draw_gameplay_feedback(framebuffer, world);
    }
    if layers.perception {
        apply_lucid_perception(framebuffer, world)?;
    }
    if let Some(reading) = reading {
        draw_oxygen_hud(framebuffer, reading);
    }
    // Marcadores de navegacion y notas de la presentacion (solo con el espacio procedural): despues de la
    // percepcion para que sigan visibles con O2 bajo, antes del final. El O2 lo lee SOLO el HUD de arriba.
    if world.sectors.is_some() {
        crate::visual::markers::draw_markers(framebuffer, world);
        crate::visual::markers::draw_hud_notes(framebuffer, world);
    }
    if let Some(seconds) = layers.ending
        && let Some(summary) = EndingSummary::from_gameplay(&world.gameplay, world.time)
    {
        draw_ending(framebuffer, &summary, seconds);
    }
    Ok(())
}

fn keyboard_input(window: &Window) -> ShipInput {
    let axis = |plus: &[Key], minus: &[Key]| {
        let down = |keys: &[Key]| keys.iter().any(|&k| window.is_key_down(k));
        f64::from(i8::from(down(plus))) - f64::from(i8::from(down(minus)))
    };
    ShipInput {
        thrust: axis(&[Key::W], &[Key::S]),
        yaw: axis(&[Key::A, Key::Left], &[Key::D, Key::Right]),
        pitch: axis(&[Key::Up], &[Key::Down]),
        roll: axis(&[Key::Q], &[Key::E]),
        boost: window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift),
    }
}

fn overlay_state(world: &World) -> OverlayState {
    OverlayState {
        culling: world.toggles.frustum_culling,
        lod: world.toggles.lod,
        backface: world.toggles.backface_culling,
        orbits_frozen: world.toggles.freeze_orbits,
    }
}

fn describe(stats: &RenderStats) -> String {
    format!(
        "objetos {} (descartados {}) | vertices {} | triangulos enviados {} descartados {} recortados {} rasterizados {} | fragmentos {} | pixeles {}",
        stats.objects_submitted,
        stats.objects_culled,
        stats.vertices_processed,
        stats.triangles_submitted,
        stats.triangles_culled,
        stats.triangles_clipped,
        stats.triangles_rasterized,
        stats.fragments_evaluated,
        stats.pixels_written
    )
}

/// Juego normal: sin la comparacion de la mirada.
pub fn run_game(options: impl Into<ConfiguredGameOptions>) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    run(options, Route::default())
}

/// Modos opt-in del mismo bucle (`run`). Sustituye la lista de booleanos posicionales.
#[derive(Clone, Copy, Default)]
struct Route {
    comparison: bool,
    distant_turn: bool,
    horizontal: bool,
    solar: bool,
    lucid: bool,
    void: bool,
    perception: bool,
    /// Estrellas observadoras (`--watching-stars`); siempre con `perception`.
    watching: bool,
}

/// Escenas con las que se combina `--lucid-perception`: el juego normal o el Sol con ojo
/// (`solar`), cada uno con o sin la superficie lucida (`lucid`) y el ojo del vacio (`void`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerceptionRoute {
    pub lucid: bool,
    pub void: bool,
    pub solar: bool,
}

/// Percepcion alterada aprobada (UMB-310, variante 02) sobre el mismo bucle
/// (`--lucid-perception`): el mundo se dibuja como siempre y la variante es el ultimo paso de
/// color de cada cuadro (inicial, headless, ventana y `P`). Valida antes de cualquier IO lo mismo
/// que la linea de comandos: tamano, tiempo, O2, los guards del juego normal o del Sol con ojo y
/// `--output` `.png` (tambien sin criatura) distinto de las entradas.
pub fn run_game_with_lucid_perception(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    run_with_perception(options, route, false)
}

/// Estrellas observadoras aprobadas (UMB-310, preview02) sobre el mismo bucle
/// (`--watching-stars`, que implica `--lucid-perception`): la misma escena `route` y el mismo O2
/// manual progresivo que la percepcion; en cada cuadro (inicial, headless, ventana y `P`) los ojos
/// van sobre la escena ya dibujada, antes de overlays y paneles, y la percepcion sigue siendo el
/// ultimo paso de color. Los ojos usan el O2 MOSTRADO, la camara real y la nave real. Valida antes
/// de cualquier IO lo mismo que `run_game_with_lucid_perception`.
pub fn run_game_with_watching_stars(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    run_with_perception(options, route, true)
}

/// Guards comunes de las dos entradas con percepcion (antes de cualquier IO) y su `Route`.
fn run_with_perception(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
    watching: bool,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    if route.solar {
        validate_solar_options(&options)?;
    } else {
        validate_normal_options(&options)?;
    }
    validate_output_path(&options.output, &[])?;
    run(
        options,
        Route {
            solar: route.solar,
            lucid: route.lucid,
            void: route.void,
            perception: true,
            watching,
            ..Route::default()
        },
    )
}

/// Juego normal con la superficie lucida aprobada en Antena (`--lucid-surface`): solo cambia
/// el shader de Antena; con O2 >= 50 % el cuadro es el de siempre. Admite la criatura estatica
/// y su revelacion como el juego normal. Valida antes de cualquier IO lo mismo que la linea de
/// comandos: tamano, tiempo, O2, modos de la criatura, arranque y alias de `--output`.
pub fn run_game_with_lucid_surface(
    options: impl Into<ConfiguredGameOptions>,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    validate_normal_options(&options)?;
    run(
        options,
        Route {
            lucid: true,
            ..Route::default()
        },
    )
}

/// Guards del juego normal sin tocar archivos: modos de la criatura, arranque y alias de
/// `--output`.
fn validate_normal_options(options: &GameOptions) -> Result<(), String> {
    let has_creature = options.creature.is_some();
    validate_reveal_flags(options.creature_reveal, has_creature, false, false)?;
    validate_staged_flags(
        options.creature_reveal_stages,
        has_creature,
        options.creature_reveal,
        false,
    )?;
    validate_eye_follow_flags(
        options.creature_eye_follow,
        options.creature_reveal_stages,
        has_creature,
        false,
    )?;
    if let Some(creature) = &options.creature {
        if creature.near && options.near.is_some() {
            return Err(
                "--near y --near-creature son incompatibles: elige un solo arranque".into(),
            );
        }
        validate_output_path(&options.output, &[&creature.model, &creature.texture])?;
    }
    Ok(())
}

/// HUD-01: opciones del HUD de oxigeno de `ConfiguredGameOptions` (que `GameOptions` no ve por `Deref`), sin
/// tocar archivos. Cada entrada publica lo llama al empezar, ANTES de su primer IO (rutas de entrada/salida, replay
/// o carga), y el nucleo lo repite como defensa. La reserva inicial solo con el O2 real (sin `--debug-oxygen`).
fn validate_configured_hud(options: &ConfiguredGameOptions) -> Result<(), String> {
    validate_oxygen_hud_flags(
        options.oxygen_hud,
        false,
        !matches!(options.oxygen_source, OxygenSource::Real),
    )
}

/// Tamano, tiempo y O2 con el contrato de la linea de comandos (`--width`/`--height` positivos
/// y dentro del maximo del framebuffer, `--time` finito >= 0, `--debug-oxygen` en `[0, 1]`).
fn validate_command_line_values(options: &GameOptions) -> Result<(), String> {
    let (width, height) = (options.width, options.height);
    if width == 0 {
        return Err("--width debe ser mayor que cero".into());
    }
    if height == 0 {
        return Err("--height debe ser mayor que cero".into());
    }
    let max = crate::renderer::framebuffer::MAX_PIXELS;
    if width.checked_mul(height).is_none_or(|n| n > max) {
        return Err(format!("{width}x{height} (maximo {max} pixeles)"));
    }
    if !(options.time.is_finite() && options.time >= 0.0) {
        return Err("--time necesita segundos >= 0".into());
    }
    if !(0.0..=1.0).contains(&options.debug_oxygen) {
        return Err(format!(
            "--debug-oxygen necesita un valor entre 0 y 1: {}",
            options.debug_oxygen
        ));
    }
    Ok(())
}

/// Juego con la comparacion A/B de la mirada (`--creature-eye-compare`): arranca mirando a la
/// camara, `G` alterna nave/camara y un panel inferior muestra el estado (tambien headless).
/// Valida antes de cualquier IO que la mirada (y por tanto etapas y criatura) este pedida.
pub fn run_game_with_eye_comparison(
    options: impl Into<ConfiguredGameOptions>,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_comparison_options(&options)?;
    run(
        options,
        Route {
            comparison: true,
            ..Route::default()
        },
    )
}

/// Candidata del giro global lejano (`--creature-distant-turn`, que exige la comparacion): la
/// comparacion entera mas el giro, arrancando en el preset `LEJOS`; `H` enciende/apaga el
/// giro, `7`/`8` colocan los presets CERCA/LEJOS y `R` repite el ultimo. Valida antes de
/// cualquier IO con los mismos guards que la comparacion.
pub fn run_game_with_distant_turn(options: impl Into<ConfiguredGameOptions>) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_comparison_options(&options)?;
    run(
        options,
        Route {
            comparison: true,
            distant_turn: true,
            ..Route::default()
        },
    )
}

/// Candidata 02 del seguimiento corporal horizontal (`--creature-horizontal-follow`, que exige la
/// comparacion y no se apila con el giro global de la 01): la comparacion entera mas el
/// seguimiento, arrancando en el preset `LEJOS`; `H` enciende/apaga (apagado vuelve al frente),
/// `7`/`8` colocan CERCA/LEJOS y `R` repite el ultimo. Valida antes de cualquier IO con los mismos
/// guards que la comparacion.
pub fn run_game_with_horizontal_follow(
    options: impl Into<ConfiguredGameOptions>,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_comparison_options(&options)?;
    run(
        options,
        Route {
            comparison: true,
            horizontal: true,
            ..Route::default()
        },
    )
}

/// Sol con ojo (`--creature-solar-reveal`): la composicion aprobada del preview 03 en el juego
/// real (`visual::solar_game`). Las etapas, la mirada, la comparacion y el seguimiento 02 van
/// implicitos (`creature_reveal_stages`/`creature_eye_follow` se ignoran). Valida antes de
/// cualquier IO con los mismos guards que la linea de comandos: criatura sin `near`, sin V1 ni
/// `--near`, y alias de `--output`. Sin panel por defecto: el estado va en el titulo.
pub fn run_game_with_solar_reveal(options: impl Into<ConfiguredGameOptions>) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_solar_options(&options)?;
    run(
        options,
        Route {
            solar: true,
            ..Route::default()
        },
    )
}

/// Sol con ojo con la superficie lucida aprobada en Antena (`--creature-solar-reveal
/// --lucid-surface`): la misma composicion y los mismos guards que `run_game_with_solar_reveal`,
/// mas los de tamano, tiempo y O2 de la linea de comandos, todo antes de cualquier IO.
pub fn run_game_with_solar_and_lucid_surface(
    options: impl Into<ConfiguredGameOptions>,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    validate_solar_options(&options)?;
    run(
        options,
        Route {
            solar: true,
            lucid: true,
            ..Route::default()
        },
    )
}

/// Ojo del vacio aprobado (UMB-309, variante 02) en Vigilia (`--void-eye`), sobre el juego
/// normal (`solar = false`) o el Sol con ojo (`solar = true`), con o sin la superficie lucida en
/// Antena (`lucid`). Valida antes de cualquier IO lo mismo que la linea de comandos en cada
/// ruta: tamano, tiempo, O2 y alias de `--output`, mas los guards del juego normal o del Sol con
/// ojo.
pub fn run_game_with_void_eye(
    options: impl Into<ConfiguredGameOptions>,
    lucid: bool,
    solar: bool,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    if solar {
        validate_solar_options(&options)?;
    } else {
        validate_normal_options(&options)?;
    }
    run(
        options,
        Route {
            solar,
            lucid,
            void: true,
            ..Route::default()
        },
    )
}

/// Guards del Sol con ojo sin tocar archivos: criatura sin `near`, sin V1 ni `--near`, y alias
/// de `--output`.
fn validate_solar_options(options: &GameOptions) -> Result<(), String> {
    validate_solar_reveal_flags(
        true,
        SolarRevealRequest {
            has_creature: options.creature.is_some(),
            near_body: options.near.is_some(),
            near_creature: options.creature.as_ref().is_some_and(|c| c.near),
            uniform_reveal: options.creature_reveal,
            ..SolarRevealRequest::default()
        },
    )?;
    if let Some(creature) = &options.creature {
        validate_output_path(&options.output, &[&creature.model, &creature.texture])?;
    }
    Ok(())
}

/// Limite de `dt` de la fisica: evita saltos enormes tras arrastrar la ventana o pausar.
pub const MAX_PHYSICS_DT: f64 = 0.1;

/// Un cuadro del juego (todos los modos), en el orden de siempre: fisica y camara con el `dt`
/// limitado; despues la revelacion y la mirada (con el seguimiento o el giro antes del ojo) con
/// los segundos **reales**, sin el limite y sin que `F5` los congele.
pub fn advance_frame(world: &mut World, input: &ShipInput, elapsed: f64) {
    world.update(input, elapsed.min(MAX_PHYSICS_DT));
    if world.gameplay.game_over || world.gameplay.objective_complete {
        return;
    }
    world.update_creature_reveal(elapsed);
    world.update_creature_eye_follow(elapsed);
}

/// Un cuadro con `--lucid-perception`: las teclas de O2 cambian el objetivo, el O2 mostrado lo
/// persigue con los segundos REALES y pasa a `world.debug_oxygen01` (Sol, superficie lucida,
/// revelacion y postproceso ven el mismo valor gradual); despues el cuadro de siempre. `F5` no
/// congela la transicion. Atomico: con teclas o segundos invalidos devuelve `Err` sin tocar ni la
/// transicion ni el mundo.
pub fn advance_perception_frame(
    world: &mut World,
    oxygen: &mut OxygenTransition,
    keys: OxygenKeys,
    input: &ShipInput,
    elapsed: f64,
) -> Result<(), String> {
    advance_perception_frame_owned(world, oxygen, keys, input, elapsed, false, false)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_perception_frame_owned(
    world: &mut World,
    oxygen: &mut OxygenTransition,
    keys: OxygenKeys,
    input: &ShipInput,
    elapsed: f64,
    captured: bool,
    body_held: bool,
) -> Result<(), String> {
    let mut next = *oxygen;
    next.press(keys)?;
    next.advance(0.0)?;
    // Validate elapsed before any world mutation. Physics consumes the clamped dt.
    let mut checked = next;
    checked.advance(elapsed)?;
    if world.gameplay.game_over || world.gameplay.objective_complete {
        return Ok(());
    }
    world.update_with_pose_owner(input, elapsed.min(MAX_PHYSICS_DT), captured);
    if world.gameplay.game_over || world.gameplay.objective_complete {
        return Ok(());
    }
    if keys.preset.is_some() || keys.increase || keys.decrease {
        world.forced_oxygen = Some(next.target());
    }
    next.press(OxygenKeys {
        preset: Some(world.forced_oxygen.unwrap_or(world.gameplay.oxygen01)),
        ..OxygenKeys::default()
    })?;
    let shown = next.advance(elapsed)?;
    *oxygen = next;
    world.publish_visual_oxygen(shown);
    // Presentacion: con un objetivo manual (1-4, M/N), el O2 REAL sigue al mostrado. Al llegar
    // la pantalla a negro (O2 0) el real tambien es 0 y el siguiente cuadro es la muerte.
    // Al alcanzar el objetivo, el forzado se suelta: desde ahi el mostrado vuelve a seguir al
    // real, asi los tanques suben el O2 (y los ojos se van) aunque antes se usara N.
    if world.oxygen_keys_drive_real
        && let Some(target) = world.forced_oxygen
    {
        world.gameplay.oxygen01 = shown;
        if shown == target {
            world.forced_oxygen = None;
        }
    }
    world.update_creature_reveal(elapsed);
    if body_held {
        world.update_creature_eye_follow_body_held(elapsed);
    } else {
        world.update_creature_eye_follow(elapsed);
    }
    Ok(())
}

/// Composicion OBSERVADORA del tentaculo en el Sol con ojo (`--tentacle-capture-debug`, opt-in de
/// debug; NO es una captura jugable): el mismo bucle que la ruta solar pedida (`route`, con o sin
/// superficie lucida, ojo del vacio, percepcion y estrellas) mas la composicion de
/// `visual::tentacle_solar_game` (F7 activa/cancela). Valida antes de cualquier IO lo mismo que esas
/// rutas: tamano, tiempo, O2, guards del Sol con ojo, `--output` (`.png` con percepcion) y las rutas y
/// el reloj del pedido. Nunca escribe nave, camara, input, O2 ni el reloj del mundo.
pub fn run_game_with_tentacle_capture(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
    perception: bool,
    watching: bool,
    capture: TentacleCaptureRequest,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    if !route.solar {
        return Err(
            "--tentacle-capture-debug requiere --creature-solar-reveal (composicion observadora en la ruta solar)"
                .into(),
        );
    }
    if watching && !perception {
        return Err("--watching-stars implica la percepcion: perception debe ser true".into());
    }
    validate_solar_options(&options)?;
    if perception {
        validate_output_path(&options.output, &[])?;
    }
    capture.validate()?;
    run_with_tentacle(
        options,
        Route {
            solar: true,
            lucid: route.lucid,
            void: route.void,
            perception,
            watching,
            ..Route::default()
        },
        Some(capture),
        TentacleRoute::Observer,
    )
}

/// PERSECUCION VIVA del tentaculo (`--tentacle-live-pursuit`, prototipo opt-in; NO es captura): el
/// mismo bucle que la ruta solar pedida (`route`, con o sin superficie lucida, ojo del vacio, percepcion
/// y estrellas) mas `visual::tentacle_live_pursuit`: con O2 <= 15 % la punta persigue la posicion
/// ACTUAL de la nave (15/10/5 %, valores candidatos) y al recuperar O2 vuelve al reposo. Valida antes de
/// cualquier IO lo mismo que la composicion observadora; protege las MISMAS entradas efectivas
/// (SC01-MTL) antes de cargar y antes de cada guardado. Nunca escribe nave, camara, input, O2 ni reloj.
pub fn run_game_with_tentacle_live_pursuit(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
    perception: bool,
    watching: bool,
    request: LivePursuitRequest,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    if !route.solar {
        return Err(
            "--tentacle-live-pursuit requiere --creature-solar-reveal (persecucion en la ruta solar)"
                .into(),
        );
    }
    if watching && !perception {
        return Err("--watching-stars implica la percepcion: perception debe ser true".into());
    }
    validate_solar_options(&options)?;
    if perception {
        validate_output_path(&options.output, &[])?;
    }
    request.validate()?;
    run_with_tentacle(
        options,
        Route {
            solar: true,
            lucid: route.lucid,
            void: route.void,
            perception,
            watching,
            ..Route::default()
        },
        Some(request.capture_request()),
        TentacleRoute::Live,
    )
}

/// PERSECUCION ESPACIAL 3D del tentaculo (`--tentacle-spatial-pursuit`, prototipo opt-in; NO es
/// captura): el mismo bucle y las mismas validaciones previas a cualquier IO que la persecucion viva, con
/// la deformacion espacial (`visual::tentacle_spatial_pursuit`). Protege las MISMAS entradas efectivas
/// (SC01-MTL) antes de cargar y antes de cada guardado. Nunca escribe nave, camara, input, O2 ni reloj.
pub fn run_game_with_tentacle_spatial_pursuit(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
    perception: bool,
    watching: bool,
    request: LivePursuitRequest,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    if !route.solar {
        return Err(
            "--tentacle-spatial-pursuit requiere --creature-solar-reveal (persecucion en la ruta solar)"
                .into(),
        );
    }
    if watching && !perception {
        return Err("--watching-stars implica la percepcion: perception debe ser true".into());
    }
    validate_solar_options(&options)?;
    if perception {
        validate_output_path(&options.output, &[])?;
    }
    request.validate()?;
    run_with_tentacle(
        options,
        Route {
            solar: true,
            lucid: route.lucid,
            void: route.void,
            perception,
            watching,
            ..Route::default()
        },
        Some(request.capture_request()),
        TentacleRoute::Spatial,
    )
}

/// CAPTURA AUTOMATICA por contacto (`--tentacle-auto-capture`, GT3; opt-in, activa en el lanzador del juego
/// privado): el mismo bucle que la ruta solar pedida con la persecucion espacial existente y, al ALCANZAR la
/// nave, el gesto aprobado (B1/P1), el arrastre y la retencion aprobados con el vuelo bloqueado desde el
/// contacto (`visual::tentacle_auto_capture`). Requiere la percepcion (O2 gradual). Valida ANTES de cualquier
/// IO lo mismo que la persecucion espacial y, con `replay`, la carpeta del replay (existente y vacia).
pub fn run_game_with_tentacle_auto_capture(
    options: impl Into<ConfiguredGameOptions>,
    route: PerceptionRoute,
    perception: bool,
    watching: bool,
    request: LivePursuitRequest,
    replay: Option<PathBuf>,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    validate_configured_hud(&options)?;
    validate_command_line_values(&options)?;
    if !route.solar {
        return Err(
            "--tentacle-auto-capture requiere --creature-solar-reveal (captura en la ruta solar)"
                .into(),
        );
    }
    if !perception {
        return Err(
            "--tentacle-auto-capture requiere --lucid-perception o --watching-stars (O2 gradual)"
                .into(),
        );
    }
    if watching && !perception {
        return Err("--watching-stars implica la percepcion: perception debe ser true".into());
    }
    validate_solar_options(&options)?;
    validate_output_path(&options.output, &[])?;
    request.validate()?;
    if let Some(dir) = &replay {
        if options.frame_limit.is_some() {
            return Err(
                "--auto-capture-replay no se combina con --frames: requiere completar Done".into(),
            );
        }
        if options.headless {
            return Err("--auto-capture-replay no se combina con --headless".into());
        }
        auto_replay::validate_dir(dir)?;
    }
    run_with_tentacle_io(
        options,
        Route {
            solar: true,
            lucid: route.lucid,
            void: route.void,
            perception,
            watching,
            ..Route::default()
        },
        Some(request.capture_request()),
        TentacleRoute::Auto,
        replay,
    )
}

/// Guards de la comparacion (y de la candidata), sin tocar archivos: mirada, etapas, criatura,
/// modos de revelacion y alias de `--output`.
fn validate_comparison_options(options: &GameOptions) -> Result<(), String> {
    validate_eye_compare_flags(true, options.creature_eye_follow, false)?;
    validate_eye_follow_flags(
        options.creature_eye_follow,
        options.creature_reveal_stages,
        options.creature.is_some(),
        false,
    )?;
    validate_staged_flags(
        options.creature_reveal_stages,
        options.creature.is_some(),
        options.creature_reveal,
        false,
    )?;
    if let Some(creature) = &options.creature {
        validate_output_path(&options.output, &[&creature.model, &creature.texture])?;
    }
    Ok(())
}

/// Panel de la comparacion sobre el cuadro ya dibujado (y el overlay, si lo hay).
fn draw_comparison(framebuffer: &mut Framebuffer, world: &World) {
    if let Some(status) = world.creature_eye_comparison_status() {
        draw_eye_comparison_panel(framebuffer, &eye_comparison_lines(&status));
    }
}

/// Panel del giro global (candidata), junto al de la comparacion.
fn draw_distant_turn(framebuffer: &mut Framebuffer, world: &World, last_preset: CreaturePreset) {
    let (Some(eye), Some(turn)) = (
        world.creature_eye_comparison_status(),
        world.creature_distant_turn_status(),
    ) else {
        return;
    };
    let left = eye_comparison_panel(
        framebuffer.width,
        framebuffer.height,
        &eye_comparison_lines(&eye),
    );
    draw_distant_turn_panel(framebuffer, &distant_turn_lines(&turn, last_preset), &left);
}

/// Panel del seguimiento horizontal (candidata 02), con el layout generico de la 01.
fn draw_horizontal_follow(
    framebuffer: &mut Framebuffer,
    world: &World,
    last_preset: CreaturePreset,
) {
    let (Some(eye), Some(follow)) = (
        world.creature_eye_comparison_status(),
        world.creature_horizontal_follow_status(),
    ) else {
        return;
    };
    let left = eye_comparison_panel(
        framebuffer.width,
        framebuffer.height,
        &eye_comparison_lines(&eye),
    );
    draw_distant_turn_panel(
        framebuffer,
        &horizontal_follow_lines(&follow, last_preset),
        &left,
    );
}

fn run(options: impl Into<ConfiguredGameOptions>, route: Route) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    run_with_tentacle(options, route, None, TentacleRoute::Observer)
}

// Identidad por handle, no por nombre: detecta tambien hardlinks. Sin dependencias nuevas.
mod capture_output {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        io,
        path::Path,
    };

    #[derive(Clone, Copy, PartialEq, Eq)]
    #[repr(C)]
    struct Identity {
        volume: u64,
        file: [u8; 16],
    }

    #[cfg(windows)]
    fn identity(file: &File) -> io::Result<Identity> {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandleEx(
                handle: *mut std::ffi::c_void,
                class: i32,
                info: *mut std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        let mut info = Identity {
            volume: 0,
            file: [0; 16],
        };
        // SAFETY: File posee un handle vivo; FILE_ID_INFO es repr(C), u64 + FILE_ID_128.
        // FileIdInfo = 0x12. El buffer escribible tiene el tamano ABI exacto (24 bytes).
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                0x12,
                (&mut info as *mut Identity).cast(),
                size_of::<Identity>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info)
    }

    #[cfg(unix)]
    fn identity(file: &File) -> io::Result<Identity> {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        Ok(Identity {
            volume: m.dev(),
            file: (m.ino() as u128).to_le_bytes(),
        })
    }

    #[cfg(not(any(windows, unix)))]
    fn identity(_: &File) -> io::Result<Identity> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "identidad de archivo no soportada",
        ))
    }

    struct Input {
        path: PathBuf,
        // Mantener abiertos: no reciclar IDs mientras exista el guard.
        _handle: File,
        id: Identity,
    }

    pub(super) struct Guard {
        inputs: Vec<Input>,
    }

    impl Guard {
        pub(super) fn new(
            request: &TentacleCaptureRequest,
            options: &GameOptions,
        ) -> Result<Self, String> {
            use crate::visual::tentacle_growth_cpu::capture::CaptureManifest;
            request.validate()?;
            let manifest_path = request.capture_dir.join("capture_package.txt");
            // Fijar primero el manifest antes de leer sus nombres dependientes.
            let mut paths = vec![
                manifest_path.clone(),
                request.growth_package.clone(),
                request.correspondence.clone(),
            ];
            if let Some(c) = &options.creature {
                paths.push(c.texture.clone());
            }
            let mut guard = Self { inputs: Vec::new() };
            for path in paths {
                guard.pin(path)?;
            }
            // Entradas efectivas del bucle, no solo flags: load_model lee OBJ y sus MTL.
            // Clasificar una sola vez, fijando cada OBJ ANTES de descubrir dependencias.
            guard.pin_model(&ship_model_path())?;
            if let Some(c) = &options.creature {
                guard.pin_model(&c.model)?;
            }
            let text = std::fs::read_to_string(&manifest_path)
                .map_err(|e| format!("capture_package.txt: {e}"))?;
            let manifest = CaptureManifest::parse(&text)?;
            for f in manifest.files() {
                guard.pin(request.capture_dir.join(&f.relative))?;
            }
            guard.preflight(&options.output)?;
            Ok(guard)
        }

        fn pin_model(&mut self, path: &Path) -> Result<(), String> {
            self.pin(path.to_path_buf())?;
            let source = std::fs::read_to_string(path)
                .map_err(|e| format!("proteccion de modelo {}: {e}", path.display()))?;
            // Reutilizar el parser congelado: comentarios, mtllib y errores son los mismos.
            let model = crate::assets::obj::parse_obj(&source)?;
            for filename in &model.material_libraries {
                let relative = Path::new(filename);
                // Mismo dominio relativo que assets::obj::load_model, sin recorrer arboles.
                if relative
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
                {
                    return Err("El MTL debe estar dentro de la carpeta del modelo".into());
                }
                self.pin(
                    path.parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(relative),
                )?;
            }
            // El loader no consume map_Kd ni otras texturas de MTL: no descubrirlas aqui.
            Ok(())
        }

        fn pin(&mut self, path: PathBuf) -> Result<(), String> {
            let mut open = OpenOptions::new();
            open.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // FILE_SHARE_READ: rechaza escritores y borrados de estos archivos abiertos.
                open.share_mode(1);
            }
            let file = open
                .open(&path)
                .map_err(|e| format!("proteccion de entrada {}: {e}", path.display()))?;
            let id = identity(&file)
                .map_err(|e| format!("identidad de entrada {}: {e}", path.display()))?;
            self.inputs.push(Input {
                path,
                _handle: file,
                id,
            });
            Ok(())
        }

        // Se repite antes de CADA guardado headless/P, no solo al arrancar.
        pub(super) fn preflight(&self, output: &Path) -> Result<(), String> {
            let paths: Vec<&Path> = self.inputs.iter().map(|i| i.path.as_path()).collect();
            validate_output_path(output, &paths)?;
            let file = match File::open(output) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(format!("preflight de salida {}: {e}", output.display())),
            };
            let id = identity(&file)
                .map_err(|e| format!("identidad de salida {}: {e}", output.display()))?;
            if let Some(input) = self.inputs.iter().find(|i| i.id == id) {
                return Err(format!(
                    "--output alias de entrada protegida (identidad de archivo): {}",
                    input.path.display()
                ));
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod input_graph_tests {
        use super::*;

        fn path(key: &str) -> PathBuf {
            std::env::var_os(key).map(PathBuf::from).expect(key)
        }

        fn setup() -> (TentacleCaptureRequest, GameOptions) {
            (
                TentacleCaptureRequest {
                    capture_dir: path("UMBRAL_CAPTURE_PACKAGE_DIR"),
                    growth_package: path("UMBRAL_GROWTH_PACKAGE"),
                    correspondence: path("UMBRAL_ARM_CORRESPONDENCE"),
                    start_time: None,
                },
                GameOptions {
                    width: 96,
                    height: 54,
                    headless: true,
                    frame_limit: Some(1),
                    output: std::env::temp_dir().join("sc01_graph_readonly_output.png"),
                    time: 0.0,
                    near: None,
                    culling: true,
                    lod: true,
                    overlay: false,
                    creature: Some(CreatureOptions {
                        model: path("UMBRAL_CAPTURE_CREATURE_OBJ"),
                        texture: path("UMBRAL_CREATURE_TEXTURE"),
                        near: false,
                    }),
                    creature_reveal: false,
                    creature_reveal_stages: true,
                    creature_eye_follow: true,
                    debug_oxygen: 0.05,
                },
            )
        }

        // Preservation guards: synthetic disposable model, no private inputs or writer on assets.
        fn disposable_model() -> (PathBuf, PathBuf) {
            let dir = std::env::temp_dir().join(format!(
                "sc01_graph_unit_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(dir.join("nested")).unwrap();
            let obj = dir.join("model.obj");
            std::fs::write(
                &obj,
                "mtllib nested/first.png # comment\nmtllib nested/second.png\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
            ).unwrap();
            for name in ["first.png", "second.png"] {
                std::fs::write(
                    dir.join("nested").join(name),
                    "newmtl test\nKd 1 0 0\nmap_Kd does_not_exist.png\n",
                )
                .unwrap();
            }
            (dir, obj)
        }

        #[test]
        fn sc01_graph_repeated_preflight_and_read_sharing() {
            let (dir, obj) = disposable_model();
            let target = dir.join("nested/second.png");
            let bytes = std::fs::read(&target).unwrap();
            let mut guard = Guard { inputs: Vec::new() };
            guard.pin_model(&obj).unwrap();
            assert_eq!(
                guard.inputs.len(),
                3,
                "OBJ plus both MTL only; no map_Kd discovery"
            );
            let output = dir.join("output.png");
            guard.preflight(&output).unwrap();
            // Late alias AFTER startup: P/headless preflight must not cache a safe output.
            std::fs::hard_link(&target, &output).unwrap();
            for _ in 0..2 {
                let error = guard.preflight(&output).unwrap_err();
                assert!(error.contains("identidad de archivo"));
                assert!(guard.preflight(&target).is_err());
            }
            #[cfg(windows)]
            assert!(
                OpenOptions::new().write(true).open(&target).is_err(),
                "MTL handle must deny writer sharing"
            );
            assert_eq!(std::fs::read(&target).unwrap(), bytes);
            drop(guard);
            // RAII releases the handle; this is only a synthetic disposable file.
            OpenOptions::new().write(true).open(&target).unwrap();
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn sc01_graph_relative_guards_match_loader() {
            let (dir, obj) = disposable_model();
            for name in ["../outside.mtl", "./nested/first.png", "/outside.mtl"]
                .into_iter()
                .chain(cfg!(windows).then_some("C:\\outside.mtl"))
            {
                let text = format!("mtllib {name}\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
                std::fs::write(&obj, text).unwrap();
                let expected = load_model(&obj).unwrap_err();
                let mut guard = Guard { inputs: Vec::new() };
                let error = guard.pin_model(&obj).unwrap_err();
                assert_eq!(error, expected);
                assert_eq!(
                    guard.inputs.len(),
                    1,
                    "invalid reference rejected without opening a dependency"
                );
                drop(guard);
            }
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        #[ignore = "private inputs; preflight only, never writer on frozen assets"]
        fn sc01_graph_builtin_and_effective_inputs() {
            use crate::assets::obj::load_model;
            use crate::visual::tentacle_growth_cpu::capture::CaptureManifest;
            let (r, o) = setup();
            let guard = Guard::new(&r, &o).unwrap();
            let ship = ship_model_path();
            let c = o.creature.as_ref().unwrap();
            let mut expected = vec![
                ship.clone(),
                c.model.clone(),
                c.texture.clone(),
                r.capture_dir.join("capture_package.txt"),
                r.growth_package.clone(),
                r.correspondence.clone(),
            ];
            for model in [&ship, &c.model] {
                for lib in load_model(model).unwrap().material_libraries {
                    expected.push(model.parent().unwrap().join(lib));
                }
            }
            let text = std::fs::read_to_string(r.capture_dir.join("capture_package.txt")).unwrap();
            for f in CaptureManifest::parse(&text).unwrap().files() {
                expected.push(r.capture_dir.join(&f.relative));
            }
            let actual: std::collections::BTreeSet<_> =
                guard.inputs.iter().map(|i| i.path.clone()).collect();
            let expected: std::collections::BTreeSet<_> = expected.into_iter().collect();
            assert_eq!(actual, expected, "SC01 graph omits effective loader input");
            // Read-only rejection, never pass immutable builtin paths to save_png.
            for model in [&ship, &c.model] {
                assert!(guard.preflight(model).is_err());
                for lib in load_model(model).unwrap().material_libraries {
                    assert!(guard.preflight(&model.parent().unwrap().join(lib)).is_err());
                }
            }
        }
    }
}

/// Que hace `run_with_tentacle` con el pedido del tentaculo (mismos paquetes y misma proteccion).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TentacleRoute {
    /// Composicion observadora (F7).
    Observer,
    /// Persecucion viva (familia plana).
    Live,
    /// Persecucion espacial 3D.
    Spatial,
    /// Persecucion espacial + captura automatica por contacto (GT3).
    Auto,
}

/// Persecucion activa del bucle (una de las dos familias; misma interfaz).
enum Pursuit {
    Live(Box<LivePursuit>),
    Spatial(Box<SpatialPursuit>),
}

impl Pursuit {
    fn step(&mut self, world: &World, dt: f64) -> Result<(), String> {
        match self {
            Self::Live(p) => p.step(world, dt).map(|_| ()),
            Self::Spatial(p) => p.step(world, dt).map(|_| ()),
        }
    }
    /// Nueva partida: el mismo arranque que `run` (bind con el World actual + paso 0).
    fn restart(&mut self, world: &World) -> Result<(), String> {
        match self {
            Self::Live(p) => {
                p.bind(world)?;
                p.step(world, 0.0).map(|_| ())
            }
            Self::Spatial(p) => {
                p.bind(world)?;
                p.step(world, 0.0).map(|_| ())
            }
        }
    }
    fn body_override(&self) -> Option<crate::scene::world::SolarBodyOverride<'_>> {
        match self {
            Self::Live(p) => p.body_override(),
            Self::Spatial(p) => p.body_override(),
        }
    }
    fn title(&self) -> String {
        match self {
            Self::Live(p) => p.title(),
            Self::Spatial(p) => p.title(),
        }
    }
}

/// El bucle de siempre; `capture` (solo con el Sol con ojo) anade la composicion observadora o, segun
/// `kind`, la persecucion viva o la espacial (mismos paquetes y la misma proteccion de entradas).
fn run_with_tentacle(
    options: impl Into<ConfiguredGameOptions>,
    route: Route,
    capture_request: Option<TentacleCaptureRequest>,
    kind: TentacleRoute,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    run_with_tentacle_io(options, route, capture_request, kind, None)
}

/// Origen de teclas, tiempo y presentacion del bucle del juego: la ventana minifb o el replay guionado de
/// la captura automatica. El CUERPO del bucle es el mismo para ambos (GT3-AUTO).
trait Frontend {
    fn is_open(&self) -> bool;
    fn down(&self, key: Key) -> bool;
    fn pressed(&self, key: Key) -> bool;
    fn ship_input(&self) -> ShipInput;
    /// Segundos reales desde el cuadro anterior; se llama UNA vez al empezar cada cuadro.
    fn elapsed(&mut self) -> f64;
    fn set_title(&mut self, title: &str);
    /// Fin del cuadro: la ventana presenta los pixeles; el replay registra y guarda sus capturas.
    fn present(&mut self, frame: &LoopFrame<'_>) -> Result<(), String>;
    /// Linea final del bucle.
    fn summary(&self, frames: usize) -> String;
}

/// Lo que el cuerpo del bucle publica al terminar un cuadro (ya con overlay y percepcion).
struct LoopFrame<'a> {
    framebuffer: &'a Framebuffer,
    world: &'a World,
    stats: &'a RenderStats,
    elapsed: f64,
    auto: Option<&'a crate::visual::tentacle_auto_capture::AutoStatus>,
    auto_frame: Option<&'a crate::visual::tentacle_auto_capture::AutoFrame>,
    contact_error_obj: Option<f64>,
}

struct WindowIo {
    window: Window,
    last: Instant,
}

impl WindowIo {
    fn new(width: usize, height: usize) -> Result<Self, String> {
        let mut window = Window::new("UMBRAL", width, height, WindowOptions::default())
            .map_err(|e| e.to_string())?;
        window.set_target_fps(60);
        Ok(Self {
            window,
            last: Instant::now(),
        })
    }
}

impl Frontend for WindowIo {
    fn is_open(&self) -> bool {
        self.window.is_open()
    }
    fn down(&self, key: Key) -> bool {
        self.window.is_key_down(key)
    }
    fn pressed(&self, key: Key) -> bool {
        self.window.is_key_pressed(key, KeyRepeat::No)
    }
    fn ship_input(&self) -> ShipInput {
        keyboard_input(&self.window)
    }
    fn elapsed(&mut self) -> f64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        elapsed
    }
    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }
    fn present(&mut self, frame: &LoopFrame<'_>) -> Result<(), String> {
        // minifb presenta pixeles ya calculados; no dibuja los triangulos de la escena.
        let fb = frame.framebuffer;
        self.window
            .update_with_buffer(&fb.pixels, fb.width, fb.height)
            .map_err(|e| e.to_string())
    }
    fn summary(&self, frames: usize) -> String {
        format!("Ventana: {frames} frames mostrados")
    }
}

/// Replay DETERMINISTA de la captura automatica por el MISMO bucle (`--auto-capture-replay <carpeta>`):
/// teclas guionadas y reactivas al estado publicado, 1/60 s por cuadro, sin ventana. Guarda capturas PNG y
/// `metrics.json` en una carpeta existente y VACIA, con creacion EXCLUSIVA y el preflight de entradas
/// protegidas antes de cada escritura. Si el guion no se completa (sin contacto, sin bloqueo...) es `Err`.
mod auto_replay {
    use super::*;
    use crate::visual::tentacle_auto_capture::{AutoEvent, AutoPhase};
    use std::{fs::OpenOptions, io::Write, path::Path};

    /// Paso fijo del replay (s).
    pub(super) const REPLAY_DT: f64 = 1.0 / 60.0;
    /// Tope de cuadros del guion completo.
    const MAX_FRAMES: usize = 7200;

    /// Carpeta del replay: existente, directorio y vacia (antes de cargar entradas o escribir nada).
    pub(super) fn validate_dir(dir: &Path) -> Result<(), String> {
        let meta = std::fs::symlink_metadata(dir)
            .map_err(|e| format!("--auto-capture-replay {}: {e}", dir.display()))?;
        if !meta.is_dir() {
            return Err(format!(
                "--auto-capture-replay necesita una carpeta existente (no enlace): {}",
                dir.display()
            ));
        }
        let mut entries = std::fs::read_dir(dir)
            .map_err(|e| format!("--auto-capture-replay {}: {e}", dir.display()))?;
        if entries.next().is_some() {
            return Err(format!(
                "--auto-capture-replay necesita una carpeta VACIA: {}",
                dir.display()
            ));
        }
        Ok(())
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Stage {
        /// 100 % de O2, sin tocar nada.
        Healthy,
        /// O2 al 5 % y vuelo mantenido (W + A 5 s; despues tres giros + impulso) hasta el contacto.
        Flying,
        /// Desde el contacto: W + tres giros + impulso mantenidos (+ R/7/8) hasta 2 s de retencion.
        Captured,
        /// O2 al 100 % con los controles mantenidos hasta la suelta, el vuelo libre y el re-armado.
        Recovering,
        Done,
    }

    pub(super) struct ReplayIo<'g> {
        dir: PathBuf,
        guard: &'g capture_output::Guard,
        frame: usize,
        stage: Stage,
        since: usize,
        hold_flight: bool,
        pressed: Vec<Key>,
        rows: Vec<String>,
        captures: Vec<String>,
        marks: Vec<String>,
        title: String,
        saw_pursuit: bool,
        half_drag: bool,
        released_at: Option<usize>,
    }

    fn num(v: f64) -> String {
        if v.is_finite() {
            format!("{v:?}")
        } else {
            "null".into()
        }
    }
    fn text(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    impl<'g> ReplayIo<'g> {
        pub(super) fn new(dir: PathBuf, guard: &'g capture_output::Guard) -> Result<Self, String> {
            validate_dir(&dir)?;
            Ok(Self {
                dir,
                guard,
                frame: 0,
                stage: Stage::Healthy,
                since: 0,
                hold_flight: false,
                pressed: Vec::new(),
                rows: Vec::new(),
                captures: Vec::new(),
                marks: Vec::new(),
                title: String::new(),
                saw_pursuit: false,
                half_drag: false,
                released_at: None,
            })
        }

        /// Archivo NUEVO dentro de la carpeta (creacion exclusiva) tras el preflight de entradas.
        fn reserve(&self, name: &str) -> Result<(PathBuf, std::fs::File), String> {
            let path = self.dir.join(name);
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| format!("replay: no se pudo crear {}: {e}", path.display()))?;
            Ok((path, file))
        }

        fn capture(&mut self, fb: &Framebuffer, label: &str) -> Result<(), String> {
            let name = format!("{:04}_{label}.png", self.frame);
            let path = self.dir.join(&name);
            self.guard.preflight(&path)?;
            save_png_exclusive(&path, fb)?;
            self.captures.push(format!(
                "{{\"frame\":{},\"file\":{}}}",
                self.frame,
                text(&name)
            ));
            Ok(())
        }

        fn mark(&mut self, what: &str) {
            self.marks.push(format!(
                "{{\"frame\":{},\"mark\":{}}}",
                self.frame,
                text(what)
            ));
        }

        fn finish(&mut self) -> Result<(), String> {
            let body = format!(
                "{{\"schema\":\"gt3-auto-capture-replay-v1\",\"dt\":{},\"frames\":{},\"title_last\":{},\"marks\":[{}],\"captures\":[{}],\"rows\":[\n{}\n]}}\n",
                num(REPLAY_DT),
                self.frame,
                text(&self.title),
                self.marks.join(","),
                self.captures.join(","),
                self.rows.join(",\n")
            );
            let (path, mut file) = self.reserve("metrics.json")?;
            file.write_all(body.as_bytes())
                .map_err(|e| format!("replay: {}: {e}", path.display()))?;
            self.stage = Stage::Done;
            Ok(())
        }
    }

    impl Frontend for ReplayIo<'_> {
        fn is_open(&self) -> bool {
            self.stage != Stage::Done
        }
        fn down(&self, _key: Key) -> bool {
            false
        }
        fn pressed(&self, key: Key) -> bool {
            self.pressed.contains(&key)
        }
        fn ship_input(&self) -> ShipInput {
            // Teclado real (+-1). Antes del contacto: W + A durante 5 s y despues A + Arriba + Q + Shift
            // (giro con impulso; medido en d02: con empuje SOSTENIDO la persecucion existente no alcanza).
            // Desde el contacto: W + A + Arriba + Q + Shift mantenidos (intento de escape a fondo).
            let spin = ShipInput {
                thrust: 0.0,
                yaw: 1.0,
                pitch: 1.0,
                roll: 1.0,
                boost: true,
            };
            match self.stage {
                _ if !self.hold_flight => ShipInput::default(),
                Stage::Flying if self.frame < self.since + 300 => ShipInput {
                    thrust: 1.0,
                    yaw: 1.0,
                    ..ShipInput::default()
                },
                Stage::Flying => spin,
                _ => ShipInput {
                    thrust: 1.0,
                    ..spin
                },
            }
        }
        fn elapsed(&mut self) -> f64 {
            REPLAY_DT
        }
        fn set_title(&mut self, title: &str) {
            self.title = title.to_string();
        }
        fn present(&mut self, f: &LoopFrame<'_>) -> Result<(), String> {
            let s = f.auto.ok_or("replay sin captura automatica")?;
            let w = f.world;
            let effective = f.auto_frame.map(|o| o.effective_ship);
            self.rows.push(format!(
                "{{\"frame\":{},\"elapsed\":{},\"phase\":{},\"preview\":{},\"events\":[{}],\"ship\":[{},{},{}],\"speed\":{},\"effective_thrust\":{},\"held_input\":{},\"o2\":{},\"body_yaw\":{},\"body_held\":{},\"contact_error_obj\":{},\"triangles\":{},\"rasterized\":{},\"fragments\":{}}}",
                self.frame,
                num(f.elapsed),
                text(&format!("{:?}", s.phase)),
                text(&format!("{:?}", s.carry.preview.phase)),
                s.events
                    .iter()
                    .map(|e| text(&format!("{e:?}")))
                    .collect::<Vec<_>>()
                    .join(","),
                num(w.ship.position.x),
                num(w.ship.position.y),
                num(w.ship.position.z),
                num(w.ship.speed),
                effective.map_or("null".into(), |e| num(e.thrust)),
                self.hold_flight,
                num(w.debug_oxygen01),
                w.creature_horizontal_follow()
                    .map_or("null".into(), |h| num(h.yaw())),
                s.body_held,
                f.contact_error_obj.map_or("null".into(), num),
                f.stats.triangles_submitted,
                f.stats.triangles_rasterized,
                f.stats.fragments_evaluated
            ));
            let fb = f.framebuffer;
            let events = &s.events;
            let ev = |e: &AutoEvent| events.contains(e);
            let any = |p: fn(&AutoEvent) -> bool| events.iter().any(p);
            // Guion reactivo (decide las teclas del cuadro SIGUIENTE con lo publicado en este).
            self.pressed.clear();
            match self.stage {
                Stage::Healthy => {
                    if self.frame == 0 {
                        self.capture(fb, "inicio_O2_100")?;
                        self.mark("arranque sano al 100 % de O2");
                    }
                    if s.phase != AutoPhase::Pursuit || w.ship.speed != 0.0 {
                        return Err("replay: estado inicial no sano".into());
                    }
                    if self.frame + 1 >= 30 {
                        self.stage = Stage::Flying;
                        self.since = self.frame + 1;
                        self.pressed.push(Key::Key4);
                        self.hold_flight = true;
                    }
                }
                Stage::Flying => {
                    let pursuing = s.carry.preview.pursuit.is_some_and(|p| {
                        p.step.mode == crate::visual::tentacle_live_pursuit::PursuitMode::Pursuing
                    });
                    if pursuing && !self.saw_pursuit {
                        self.saw_pursuit = true;
                        self.capture(fb, "persigue")?;
                        self.mark("la persecucion existente se activa (O2 <= 15 %)");
                    }
                    if ev(&AutoEvent::Contact) {
                        self.capture(fb, "contacto")?;
                        self.mark("contacto real: nave capturada en su pose, sin T ni quietud");
                        self.stage = Stage::Captured;
                        self.since = self.frame;
                    } else if self.frame >= self.since + 3000 {
                        return Err("replay: la punta no alcanzo la nave".into());
                    } else if (self.frame - self.since) % 240 == 120 {
                        self.capture(fb, "vuelo")?;
                    }
                }
                Stage::Captured => {
                    let k = self.frame - self.since;
                    for (at, key) in [(20, Key::R), (40, Key::Key7), (60, Key::Key8)] {
                        if k + 1 == at {
                            self.pressed.push(key);
                        }
                    }
                    if k == 30 {
                        self.capture(fb, "traspaso_B1")?;
                    }
                    if k == 200 {
                        self.capture(fb, "cierre_P1")?;
                    }
                    if ev(&AutoEvent::Locked) {
                        self.capture(fb, "agarre_bloqueo")?;
                        self.mark("Gripped: bloqueo del arrastre aprobado");
                    }
                    if s.phase == AutoPhase::Dragging && s.carry.progress >= 0.5 && !self.half_drag
                    {
                        self.half_drag = true;
                        self.capture(fb, "arrastre_mitad")?;
                    }
                    if ev(&AutoEvent::HeldReached) {
                        self.capture(fb, "retenida")?;
                        self.mark("retenida junto a la criatura");
                        self.since = self.frame;
                    }
                    if any(|e| matches!(e, AutoEvent::Released(_) | AutoEvent::Aborted(_))) {
                        return Err(format!("replay: soltada antes de recuperar O2: {events:?}"));
                    }
                    if s.phase == AutoPhase::Held && self.frame == self.since + 120 {
                        self.capture(fb, "retenida_2s")?;
                        self.stage = Stage::Recovering;
                        self.since = self.frame + 1;
                        self.pressed.push(Key::Key1);
                    } else if k > 2400 {
                        return Err("replay: no llego a RETENIDA".into());
                    }
                }
                Stage::Recovering => {
                    if any(|e| matches!(e, AutoEvent::Released(_))) {
                        self.capture(fb, "suelta_O2")?;
                        self.mark("O2 > 15 %: suelta en su pose, sin inercia");
                        self.released_at = Some(self.frame);
                    }
                    if self.released_at.is_some_and(|r| self.frame == r + 60) {
                        self.capture(fb, "vuelo_libre")?;
                    }
                    if ev(&AutoEvent::Rearmed) {
                        self.capture(fb, "rearmado")?;
                        self.mark(
                            "re-armado: la criatura se oculto y el tentaculo vuelve al reposo",
                        );
                        self.frame += 1;
                        return self.finish();
                    }
                    if self.frame > self.since + 1200 {
                        return Err("replay: sin suelta o sin re-armado tras recuperar O2".into());
                    }
                }
                Stage::Done => {}
            }
            self.frame += 1;
            if self.frame >= MAX_FRAMES {
                return Err("replay: guion sin terminar".into());
            }
            Ok(())
        }
        fn summary(&self, frames: usize) -> String {
            format!(
                "Replay de captura automatica: {frames} cuadros por el bucle del juego | {} capturas en {}",
                self.captures.len(),
                self.dir.display()
            )
        }
    }
}

/// `run_with_tentacle` con el origen del bucle elegido: la ventana o, con `replay`, el replay guionado de la
/// captura automatica (mismo cuerpo de bucle).
fn run_with_tentacle_io(
    options: impl Into<ConfiguredGameOptions>,
    route: Route,
    capture_request: Option<TentacleCaptureRequest>,
    kind: TentacleRoute,
    replay: Option<PathBuf>,
) -> Result<(), String> {
    let options: ConfiguredGameOptions = options.into();
    // HUD de oxigeno (preview): defensa central antes de cualquier archivo (las entradas publicas ya lo validan
    // al empezar, antes de su propio IO de rutas: HUD-01).
    validate_configured_hud(&options)?;
    let Route {
        comparison,
        distant_turn,
        horizontal,
        solar,
        lucid,
        void,
        perception,
        watching,
    } = route;
    let output_guard = capture_request
        .as_ref()
        .map(|request| capture_output::Guard::new(request, &options))
        .transpose()?;
    let (width, height) = (options.width, options.height);
    let ship_model = load_model(&ship_model_path())?;
    println!(
        "Modelo: {} vertices, {} caras, {} materiales",
        ship_model.vertices.len(),
        ship_model.faces.len(),
        ship_model.materials.len()
    );
    let mut framebuffer = Framebuffer::new(width, height, SPACE_COLOR).map_err(|e| {
        format!(
            "{e}: {width}x{height} (maximo {} pixeles)",
            crate::renderer::framebuffer::MAX_PIXELS
        )
    })?;
    let mut world = World::new(ship_model, width as f64 / height as f64)?;
    world.time = options.time;
    match options.oxygen_source {
        OxygenSource::Debug => world.set_debug_oxygen(options.debug_oxygen),
        OxygenSource::Real => world.clear_debug_oxygen(),
        OxygenSource::Legacy => world.debug_oxygen01 = options.debug_oxygen,
    }
    if let Some(seconds) = options.oxygen_seconds {
        set_oxygen_tank_seconds(&mut world, seconds)?;
        println!("Oxigeno: el tanque completo dura {seconds:.0} s (L alterna 420/120/60/30 s)");
    }
    if let Some(reserve) = options.oxygen_hud.initial_reserve {
        // Preview del HUD: RESERVA REAL inicial (no un override visual). El O2 visual la sigue como siempre
        // y a partir de aqui el juego la gasta y recupera con sus reglas; Shift+R vuelve a la reserva llena.
        world.gameplay.oxygen01 = reserve;
        world.clear_debug_oxygen();
    }
    if options.oxygen_hud.enabled {
        println!(
            "HUD de oxigeno: instrumento superior con la RESERVA REAL {:.0}% (baja con el tanque; en la experiencia por defecto tambien con N/M y 1-4)",
            world.gameplay.oxygen01 * 100.0
        );
    }
    world.toggles.frustum_culling = options.culling;
    world.toggles.lod = options.lod;
    if lucid {
        // Una sola vez: todas las rutas de dibujo (inicial, headless, bucle y P) usan el shader
        // del cuerpo con el tiempo y el O2 vivos del mundo.
        let index = set_lucid_surface(&mut world, true)?;
        println!(
            "Superficie lucida (aprobada 02): {} (cuerpo {index}) usa LucidSurface | O2 {:.2} (grietas bajo 50 %) | 1-4 y M/N cambian el O2",
            world.system.bodies[index].name, world.debug_oxygen01
        );
    }
    if void {
        let index = set_void_eye(&mut world, true)?;
        println!(
            "Ojo del vacio (aprobado 02): {} (cuerpo {index}) usa VoidEye",
            world.system.bodies[index].name
        );
    }
    if let Some(index) = options.near {
        world.place_near(index, 25.0)?;
    }
    if options.presentation {
        world.enable_sectors();
        world.oxygen_keys_drive_real = true;
        world.gameplay.settings.pickup_recovery = PRESENTATION_TANK_RECOVERY;
        println!(
            "Presentacion: espacio procedural por sectores, tanques de O2 visibles y marcadores; N/M y 1-4 arrastran el O2 real"
        );
    }
    // Objetivo manual y O2 mostrado: arrancan EXACTAMENTE en --debug-oxygen (sin presimular).
    let mut oxygen = if perception {
        Some(OxygenTransition::new(world.debug_oxygen01)?)
    } else {
        None
    };
    let near_creature = options.creature.as_ref().is_some_and(|c| c.near);
    let mut solar_game = None;
    if let Some(requested) = &options.creature {
        // Se carga una sola vez; el mundo toma posesion del modelo y la textura.
        let start = Instant::now();
        let (placement, place_name) = if solar {
            (solar_creature_placement(&world)?, "centro del Sol")
        } else {
            (CreaturePlacement::demo(), "ubicacion de demostracion")
        };
        let creature = StaticCreature::load(&requested.model, &requested.texture, placement)?;
        let (placement, texture) = (creature.placement(), creature.texture());
        println!(
            "Criatura: {} vertices, {} triangulos, textura {}x{} | carga {:.0} ms | centro ({}, {}, {}) radio {} ({place_name})",
            creature.model().vertices.len(),
            creature.model().faces.len(),
            texture.width(),
            texture.height(),
            start.elapsed().as_secs_f64() * 1000.0,
            placement.center().x,
            placement.center().y,
            placement.center().z,
            placement.world_radius()
        );
        if solar {
            // Composicion aprobada del preview 03: arranque completo con las APIs aceptadas.
            let game = SolarGame::start(&mut world, creature)?;
            let alphas = world.solar_reveal_alphas().ok_or("sin preview solar")?;
            let (eye_faces, body_faces) = world
                .staged_creature_parts()
                .map_or((0, 0), |p| (p.eye_faces(), p.body_faces()));
            println!(
                "Sol con ojo (composicion aprobada 03): criatura en el centro del Sol | vista FRONTAL | O2 {:.2}: sol {:.2} ojo {:.2} cuerpo {:.3} | caras ojo {eye_faces} cuerpo {body_faces} | mirada a la CAMARA, seguimiento horizontal 02, luz de camara | {}",
                world.debug_oxygen01,
                alphas.sun,
                alphas.eye,
                alphas.body,
                game.title(&world)
            );
            solar_game = Some(game);
        } else {
            world.set_creature(creature)?;
        }
        if near_creature {
            world.place_near_creature()?;
        }
        if options.creature_reveal {
            // Arranca ya en el objetivo del oxigeno inicial: cuadros headless deterministas.
            world.enable_creature_reveal(RevealSettings::default());
            let shown = world.creature_reveal().map_or(0.0, |r| r.shown());
            println!(
                "Revelacion por oxigeno: O2 {:.2} | visibilidad {:.0}%",
                world.debug_oxygen01,
                shown * 100.0
            );
        }
        if options.creature_reveal_stages && !solar {
            // Particion una vez al activar; tambien arranca ya en el objetivo del O2 inicial.
            world.enable_staged_creature_reveal(
                StagedRevealSettings::default(),
                CREATURE_EYE_REGION,
            )?;
            let (eye, body) = world
                .staged_creature_reveal()
                .map_or((0.0, 0.0), |r| r.alphas());
            let (eye_faces, body_faces) = world
                .staged_creature_parts()
                .map_or((0, 0), |p| (p.eye_faces(), p.body_faces()));
            println!(
                "Revelacion por etapas: O2 {:.2} | ojo {:.0}% cuerpo {:.1}% | caras ojo {eye_faces} cuerpo {body_faces} (region del ojo calibrada para este asset)",
                world.debug_oxygen01,
                eye * 100.0,
                body * 100.0
            );
        }
        if options.creature_eye_follow && !solar {
            // Calibracion una vez al activar; arranca ya mirando a la nave (headless determinista).
            // La comparacion arranca en la camara (ya colocada), tambien con salto determinista.
            let settings = EyeFollowSettings::default();
            let (target, name) = if comparison {
                (EyeTarget::Camera, "camara")
            } else {
                (EyeTarget::Ship, "nave")
            };
            world.enable_creature_eye_follow_targeting(settings, target)?;
            let gaze = world
                .creature_eye_follow()
                .map_or(GazeAngles::NEUTRAL, |f| f.current());
            // La candidata 02 lo imprime despues de su preset (este quedaria obsoleto).
            if !horizontal {
                println!(
                    "Mirada: yaw {:+.3} grados, pitch {:+.3} grados hacia la {name} | t95 {} s (candidato)",
                    gaze.yaw().to_degrees(),
                    gaze.pitch().to_degrees(),
                    settings.t95_seconds()
                );
            }
            if comparison {
                println!(
                    "Comparacion de la mirada: G alterna NAVE/CAMARA (prueba, no cambia limites ni tiempo)"
                );
            }
        }
        if distant_turn {
            // Candidata: arranca en el preset LEJOS (aunque se pida --near-creature), con el giro
            // en reposo y el ojo ya mirando al punto elegido desde la camara colocada.
            world.enable_creature_distant_turn()?;
            world.place_creature_preset(CreaturePreset::Far)?;
            println!(
                "Giro global (candidata): arranque LEJOS {} radios a {:.0} grados | max {:.0}/{:.0} grados | t95 {} s | peso 0 hasta {} radios, 1 desde {} | H/7/8/R",
                FAR_PRESET_RADII,
                FAR_PRESET_LATERAL.to_degrees(),
                DISTANT_TURN_MAX_YAW.to_degrees(),
                DISTANT_TURN_MAX_PITCH.to_degrees(),
                DISTANT_TURN_T95_SECONDS,
                DISTANT_TURN_NEAR_RADII,
                DISTANT_TURN_FAR_RADII
            );
        }
        if horizontal {
            // Candidata 02: arranca en el preset LEJOS (aunque se pida --near-creature), con el
            // cuerpo de frente; los diagnosticos salen despues del preset efectivo.
            world.enable_creature_horizontal_follow()?;
            world.place_creature_preset(CreaturePreset::Far)?;
            let status = world.creature_horizontal_follow_status();
            println!(
                "Seguimiento horizontal (candidata 02): arranque LEJOS {} radios a {:.0} grados | cuerpo yaw {:+.1}, objetivo {} grados (camara) | t95 {} s, tope {:.0} grados/s, solo yaw, cerca y lejos | H/7/8/R",
                FAR_PRESET_RADII,
                FAR_PRESET_LATERAL.to_degrees(),
                status.map_or(0.0, |s| s.follow.yaw().to_degrees()),
                status
                    .and_then(|s| s.target_azimuth)
                    .map_or("--".to_string(), |a| format!("{:+.1}", a.to_degrees())),
                HORIZONTAL_FOLLOW_T95_SECONDS,
                HORIZONTAL_FOLLOW_MAX_SPEED.to_degrees()
            );
            let gaze = world
                .creature_eye_follow()
                .map_or(GazeAngles::NEUTRAL, |f| f.current());
            println!(
                "Mirada (tras el preset LEJOS): yaw {:+.3} grados, pitch {:+.3} grados hacia la camara | t95 {} s (candidato)",
                gaze.yaw().to_degrees(),
                gaze.pitch().to_degrees(),
                EyeFollowSettings::default().t95_seconds()
            );
        }
    }
    // Composicion observadora (opt-in): carga y enlace UNA vez tras el arranque solar; con
    // `--tentacle-capture-time` se activa aqui (replay determinista del mismo camino que F7).
    let mut pursuit = None;
    let (mut auto_pursuit, mut auto_package, mut auto_rest) = (None, None, None);
    let capture_request = match capture_request {
        Some(request) if kind == TentacleRoute::Auto => {
            if solar_game.is_none() {
                return Err("--tentacle-auto-capture requiere el Sol con ojo".into());
            }
            // Mismos bytes leidos UNA vez para la persecucion y el clip (manifest y FNV verificados).
            let PreviewInputs { debug, package } = load_preview_inputs(&request)?;
            let mut p = SpatialPursuit::new(debug);
            let report = p.bind(&world)?;
            let status = p.step(&world, 0.0)?;
            auto_rest = Some(arm_rest(&world, &p)?);
            println!(
                "CAPTURA AUTOMATICA por contacto (GT3): persecucion espacial existente | Meshy {} v / {} caras | cuerpo {} caras - brazo {} + tramo -> {} caras | alcance {} OBJ ({:.1} u/OBJ) | al ALCANZAR la nave: gesto aprobado B1/P1, arrastre y retencion aprobados; vuelo y R/7/8 bloqueados desde el contacto; O2 > 15 % suelta | {}",
                report.capture.meshy_vertices,
                report.capture.meshy_faces,
                report.capture.body_faces_before,
                report.capture.arm_faces_removed,
                report.capture.composite_faces,
                report.max_reach_obj,
                status.world_per_obj,
                p.title()
            );
            auto_pursuit = Some(p);
            auto_package = Some(package);
            None
        }
        Some(request) if kind == TentacleRoute::Spatial => {
            if solar_game.is_none() {
                return Err("--tentacle-spatial-pursuit requiere el Sol con ojo".into());
            }
            let mut p = SpatialPursuit::new(SolarCaptureDebug::load(&request)?);
            let report = p.bind(&world)?;
            let status = p.step(&world, 0.0)?;
            println!(
                "PERSECUCION ESPACIAL 3D del tentaculo (prototipo, NO agarre): Meshy {} v / {} caras | cuerpo {} caras - brazo {} + tramo -> {} caras | raiz compartida {} | {} vertices retenidos en posicion EXACTA del Meshy | codo {:.4} OBJ, punta axial {:.4} OBJ, bola minima espacial {:.4} OBJ, alcance {} OBJ ({:.1} u/OBJ) | primer codo {} del giro plano, memoria de lado {} rad por paso angular (CANDIDATOS) | O2 <= 15 %: rapidez {:?} u/s, aceleracion {} u/s2, recogida {} u/s (CANDIDATOS) | {}",
                report.capture.meshy_vertices,
                report.capture.meshy_faces,
                report.capture.body_faces_before,
                report.capture.arm_faces_removed,
                report.capture.composite_faces,
                report.capture.shared_root_vertices,
                report.exact_rest_vertices,
                report.bend_radius_obj,
                report.tip_offset_obj,
                report.min_radius_obj,
                report.max_reach_obj,
                status.world_per_obj,
                crate::visual::tentacle_spatial_pursuit::SPATIAL_FIRST_ELBOW_FRACTION,
                crate::visual::tentacle_spatial_pursuit::SPATIAL_SIDE_RATE,
                crate::visual::tentacle_live_pursuit::PURSUIT_SPEEDS,
                crate::visual::tentacle_live_pursuit::PURSUIT_ACCELERATION,
                crate::visual::tentacle_live_pursuit::PURSUIT_RETRACT_SPEED,
                p.title()
            );
            pursuit = Some(Pursuit::Spatial(Box::new(p)));
            None
        }
        Some(request) if kind == TentacleRoute::Live => {
            if solar_game.is_none() {
                return Err("--tentacle-live-pursuit requiere el Sol con ojo".into());
            }
            let mut p = LivePursuit::new(SolarCaptureDebug::load(&request)?);
            let report = p.bind(&world)?;
            let status = p.step(&world, 0.0)?;
            println!(
                "PERSECUCION VIVA del tentaculo (prototipo, NO agarre): Meshy {} v / {} caras | cuerpo {} caras - brazo {} + tramo -> {} caras | raiz compartida {} | {} vertices retenidos en posicion EXACTA del Meshy | codo {:.4} OBJ, punta axial {:.4} OBJ, alcance {} OBJ ({:.1} u/OBJ) | O2 <= 15 %: rapidez {:?} u/s, aceleracion {} u/s2, recogida {} u/s (CANDIDATOS) | {}",
                report.capture.meshy_vertices,
                report.capture.meshy_faces,
                report.capture.body_faces_before,
                report.capture.arm_faces_removed,
                report.capture.composite_faces,
                report.capture.shared_root_vertices,
                report.exact_rest_vertices,
                report.bend_radius_obj,
                report.tip_offset_obj,
                report.max_reach_obj,
                status.world_per_obj,
                crate::visual::tentacle_live_pursuit::PURSUIT_SPEEDS,
                crate::visual::tentacle_live_pursuit::PURSUIT_ACCELERATION,
                crate::visual::tentacle_live_pursuit::PURSUIT_RETRACT_SPEED,
                p.title()
            );
            pursuit = Some(Pursuit::Live(Box::new(p)));
            None
        }
        other => other,
    };
    let mut capture = match capture_request {
        None => None,
        Some(request) => {
            if solar_game.is_none() {
                return Err("--tentacle-capture-debug requiere el Sol con ojo".into());
            }
            let mut debug = SolarCaptureDebug::load(&request)?;
            let report = debug.bind(&world)?;
            println!(
                "Composicion OBSERVADORA del tentaculo (debug, NO agarre): Meshy {} v / {} caras | cuerpo {} caras - brazo {} + tramo -> {} caras | ojo {} caras sin brazo | raiz compartida {} | desvio export {:e} | F7 activa/cancela",
                report.meshy_vertices,
                report.meshy_faces,
                report.body_faces_before,
                report.arm_faces_removed,
                report.composite_faces,
                report.eye_faces,
                report.shared_root_vertices,
                report.max_position_error
            );
            if let Some(seconds) = request.start_time {
                let snapshot = debug.activate(&world)?;
                debug.set_clock(seconds)?;
                println!(
                    "Composicion activada al arrancar (replay): objetivo {:?} | delta OBJ {:?} | factor {:.4} | duracion {:.3} s | x168 {:?} | reloj {seconds} s",
                    snapshot.target_world,
                    snapshot.delta_obj,
                    snapshot.factor,
                    snapshot.duration_s,
                    snapshot.cone_fraction_at_grip
                );
            }
            Some(debug)
        }
    };
    let auto_clip = match (&auto_package, &auto_rest) {
        (Some(package), Some(rest)) => Some(ClosureClip::new(package, rest)?),
        _ => None,
    };
    let mut auto = match (&auto_clip, auto_pursuit) {
        (Some(clip), Some(p)) => Some(AutoCapture::new(GraspPreview::new(clip, p)?)?),
        _ => None,
    };
    if let Some(a) = &auto {
        let plan = a.carry().plan();
        println!(
            "Captura automatica lista: agarre {:.4} s + arrastre {:.4} s (aprobados) | extremo {:.4} OBJ | {}",
            plan.hold_s,
            plan.drag_s,
            plan.r_end_obj,
            a.title()
        );
    }
    let mut capture_note = String::new();
    let mut last_preset = CreaturePreset::Far;
    let dispatch_route = DispatchRoute {
        comparison,
        distant_turn,
        horizontal,
        near_creature,
        near_body: options.near,
        auto: auto.is_some(),
    };
    // El modo solar dibuja siempre con su cuadro (inicial, headless y cada cuadro del bucle); con
    // `--watching-stars` el mismo helper pone los ojos sobre la escena, antes de overlays y post.
    // La capa Body alternativa (composicion activa) entra en la escena, antes de los ojos.
    let render = |world: &World,
                  framebuffer: &mut Framebuffer,
                  solar: Option<&SolarGame>,
                  body: Option<&SolarBodyOverride<'_>>| {
        render_game_frame_with(world, framebuffer, solar, watching, body)
    };
    if let Some(debug) = capture.as_mut()
        && let Some(status) = debug.update_frame(&world)?
    {
        println!(
            "Composicion: cuadro {} (base {:.4} s) | r {:.4} | recorte {} | longitud del tramo {:.6} | deriva del cuerpo {:.6}",
            status.frame,
            status.base_seconds,
            status.reach_weight,
            status.clamped,
            status.growth_length,
            status.creature_drift_world
        );
    }
    let frame = render(
        &world,
        &mut framebuffer,
        solar_game.as_ref(),
        capture
            .as_ref()
            .and_then(|c| c.body_override())
            .or_else(|| pursuit.as_ref().and_then(|p| p.body_override()))
            .or_else(|| auto.as_ref().and_then(|a| a.body_override()))
            .as_ref(),
    )?;
    let stats = frame.stats;
    println!("{}", describe(&stats));
    if let Some(eyes) = &frame.watching_stars {
        println!(
            "Estrellas observadoras (aprobadas 02): O2 mostrado {:.2} | ojos {} (activas {}, ocluidas {}) | miran a la nave proyectada con la camara real | antes de overlays y percepcion",
            eyes.oxygen01,
            eyes.eyes.len(),
            eyes.active_ids.len(),
            eyes.occluded_ids.len()
        );
    }
    if let Some(oxygen) = &oxygen {
        let focus = ship_focus(&world);
        println!(
            "Percepcion alterada (aprobada 02): ultimo paso de color | O2 {:.2} (objetivo {:.2}) | transicion lineal {OXYGEN_TRANSITION_RATE} O2/s reales, bajada y recuperacion | foco nave proyectada ({:.4}, {:.4}){} | 1-4 y M/N cambian el objetivo",
            oxygen.shown(),
            oxygen.target(),
            focus.uv[0],
            focus.uv[1],
            if focus.projected {
                ""
            } else {
                " SIN PROYECCION: centro"
            }
        );
    }
    let mut show_overlay = options.overlay;
    if options.headless {
        // Un solo cuadro: si la partida ya termino, el final se muestra completo.
        let ending_seconds = EndingKind::of(&world.gameplay).map(|_| ENDING_FULLY_SHOWN);
        let lines = show_overlay.then(|| {
            overlay_lines(
                None,
                &stats,
                &overlay_state(&world),
                world.ship.speed,
                world.debug_oxygen01,
                world.time,
            )
        });
        compose_presentation_with_hud(
            &mut framebuffer,
            &world,
            &PresentationLayers {
                overlay: lines.as_deref(),
                comparison,
                distant_turn,
                horizontal,
                preset: last_preset,
                gameplay_feedback: options.gameplay_feedback,
                perception,
                ending: ending_seconds,
            },
            options.oxygen_hud.enabled,
        )?;
        if let Some(guard) = &output_guard {
            guard.preflight(&options.output)?;
        }
        if kind == TentacleRoute::Auto {
            save_png_exclusive(&options.output, &framebuffer)?;
        } else {
            save_png(&options.output, &framebuffer)?;
        }
        println!("PNG: {} ({width}x{height})", options.output.display());
        return Ok(());
    }

    let mut io: Box<dyn Frontend + '_> = match (&replay, &output_guard) {
        (Some(dir), Some(guard)) => Box::new(auto_replay::ReplayIo::new(dir.clone(), guard)?),
        (Some(_), None) => {
            return Err("--auto-capture-replay requiere la captura automatica".into());
        }
        (None, _) => Box::new(WindowIo::new(width, height)?),
    };
    let mut fps = 60.0;
    let (mut frames, mut title_clock, mut title_frames) = (0usize, 0.0, 0u32);
    let mut ending_clock = EndingClock::new();
    // Muerte por el monstruo (presentacion con la criatura): al llegar el O2 a 0 te jala, te
    // atrapa con la captura aprobada y te mata.
    let monster_mode = options.presentation && auto.is_some();
    let mut monster = MonsterDeath::new();
    while io.is_open() && !io.down(Key::Escape) {
        let elapsed = io.elapsed();
        // Limitar dt evita saltos enormes tras arrastrar la ventana o pausar el proceso.
        let dt = elapsed.min(MAX_PHYSICS_DT);
        let mut auto_frame: Option<AutoFrame> = None;
        let shift = io.down(Key::LeftShift) || io.down(Key::RightShift);
        let mut preset = None;
        for (key, value) in [
            (Key::Key1, 1.0),
            (Key::Key2, 0.5),
            (Key::Key3, 0.25),
            (Key::Key4, 0.05),
        ] {
            if io.pressed(key) {
                preset = Some(value);
            }
        }
        // Una pulsacion por paso, sin repeticion al mantener la tecla. Los presets 1-4
        // siguen disponibles; M/N se aplican despues si coinciden en el mismo cuadro.
        let keys = FrameKeys {
            full_restart: io.pressed(Key::R) && shift,
            reset: io.pressed(Key::R) && !shift,
            frontal: io.pressed(Key::Key7),
            oblique: io.pressed(Key::Key8),
            follow: io.pressed(Key::H),
            target: io.pressed(Key::G),
            orbit_assist: io.pressed(Key::O),
            real_oxygen: io.pressed(Key::Key0),
            oxygen: OxygenKeys {
                preset,
                increase: io.pressed(Key::M),
                decrease: io.pressed(Key::N),
            },
            capture_toggle: io.pressed(Key::F7),
            emergency: io.pressed(Key::X),
        }
        .gated(&world);
        if io.pressed(Key::F3) {
            world.toggles.frustum_culling ^= true;
        }
        if io.pressed(Key::F4) {
            world.toggles.lod ^= true;
        }
        if io.pressed(Key::F1) {
            show_overlay ^= true;
        }
        if io.pressed(Key::L) {
            let seconds = cycle_oxygen_pace(&mut world);
            println!("L: el tanque completo de oxigeno dura ahora {seconds:.0} s");
        }
        if io.pressed(Key::F2) {
            world.toggles.planet_props ^= true;
        }
        if io.pressed(Key::F6) {
            world.toggles.backface_culling ^= true;
        }
        if io.pressed(Key::F5) {
            world.toggles.freeze_orbits ^= true;
        }
        // Captura automatica: R se decide en su cuadro compartido (bloqueada con la nave capturada).
        let auto_solar = dispatch_frame_keys(
            &mut world,
            solar_game.as_mut(),
            oxygen.as_mut(),
            dispatch_route,
            &mut last_preset,
            keys,
        )?;
        if keys.full_restart {
            monster.reset();
            // Estados no automaticos de la nueva partida por sus APIs existentes: la composicion se
            // cancela y la persecucion se reenlaza como al arrancar (bind + paso 0).
            if let Some(debug) = &mut capture {
                debug.cancel();
            }
            if let Some(p) = pursuit.as_mut() {
                p.restart(&world)?;
            }
            capture_note.clear();
        }
        let oxygen_keys = keys.oxygen;
        // Fisica con `dt` limitado; revelacion y mirada con los segundos reales, hacia la nave
        // ya movida (F5 no las congela). Con las candidatas avanza antes el giro global o el
        // seguimiento y el ojo los compensa.
        // Con --lucid-perception las teclas solo cambian el objetivo y el O2 mostrado avanza aqui
        // con los mismos segundos reales (sin el limite de la fisica y sin que F5 lo congele).
        if let (Some(a), Some(o2), Some(game)) =
            (auto.as_mut(), oxygen.as_mut(), solar_game.as_mut())
        {
            // GT3: el MISMO cuadro compartido que las pruebas (preflight, teclas solares filtradas, fisica con
            // la entrada efectiva, captura automatica). `?` solo con entradas invalidas: nada cambio.
            let ship_input = if monster_mode {
                let phase = a.status().phase;
                monster.before_frame(&mut world, io.ship_input(), phase)
            } else {
                io.ship_input()
            };
            let out = advance_auto_capture_frame(
                &mut world,
                game,
                o2,
                a,
                AutoControls {
                    ship: ship_input,
                    oxygen: oxygen_keys,
                    solar: auto_solar,
                    emergency: keys.emergency,
                },
                elapsed,
            )?;
            match &out.status {
                Ok(s) => {
                    capture_note.clear();
                    for e in &s.events {
                        if !matches!(e, AutoEvent::DragStarted) {
                            println!(
                                "Captura automatica [cuadro {}]: {e:?} | {}",
                                s.frame,
                                a.title()
                            );
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Captura automatica: cuadro rechazado: {error}");
                    capture_note = format!(" | cuadro rechazado: {error}");
                }
            }
            if monster_mode && monster.after_frame(&mut world, a.status().phase, elapsed) {
                println!("La Vigilia: el monstruo te arrastro y te mato (fin de la partida)");
            }
            if out.aborted {
                eprintln!("Captura automatica ABORTADA (nave libre, sin tentaculo)");
            }
            auto_frame = Some(out);
        } else {
            match &mut oxygen {
                Some(oxygen) => advance_perception_frame(
                    &mut world,
                    oxygen,
                    oxygen_keys,
                    &io.ship_input(),
                    elapsed,
                )?,
                None => advance_frame(&mut world, &io.ship_input(), elapsed),
            }
        }
        // Terminal: relojes de efecto congelados hasta Shift+R (composicion y persecucion no avanzan).
        let terminal = world.gameplay.game_over || world.gameplay.objective_complete;
        // Presentacion final: segundos REALES desde la victoria o la muerte; Shift+R lo reinicia.
        ending_clock.update(&world.gameplay, elapsed);
        let ending_seconds = ending_clock.seconds();
        if let Some(debug) = capture.as_mut() {
            // Reloj propio con segundos REALES; F7 toma la instantanea tras la fisica del cuadro.
            if !terminal {
                debug.advance(elapsed)?;
            }
            if keys.capture_toggle {
                match debug.toggle(&world) {
                    Ok(CaptureToggle::Activated(s)) => {
                        capture_note.clear();
                        println!(
                            "F7: composicion OBSERVADORA activada | objetivo {:?} | delta OBJ {:?} | factor {:.4} | duracion {:.3} s | x168 {:?}",
                            s.target_world,
                            s.delta_obj,
                            s.factor,
                            s.duration_s,
                            s.cone_fraction_at_grip
                        );
                    }
                    Ok(CaptureToggle::Cancelled) => {
                        capture_note.clear();
                        println!("F7: composicion cancelada (vuelve la criatura completa)");
                    }
                    Err(error) => {
                        eprintln!("F7 rechazado (nada cambia): {error}");
                        capture_note = format!(" | F7 rechazado: {error}");
                    }
                }
            }
            if let Err(error) = debug.update_frame(&world) {
                eprintln!("Composicion cancelada (nada se dibujo con ella): {error}");
                capture_note = format!(" | composicion cancelada: {error}");
                debug.cancel();
            }
        }
        if let Some(p) = pursuit.as_mut().filter(|_| !terminal) {
            // Tras la fisica del cuadro: posicion ACTUAL de la nave, transform ACTUAL de la criatura
            // y O2 mostrado, con el mismo `dt` limitado de la nave. Un `Err` no cambia nada: se dibuja
            // el ultimo estado publicado y se avisa (sin cancelar ni ocultar).
            match p.step(&world, dt) {
                Ok(_) => capture_note.clear(),
                Err(error) => {
                    eprintln!("Persecucion: paso rechazado (sin cambios): {error}");
                    capture_note = format!(" | paso rechazado: {error}");
                }
            }
        }
        let stats = render(
            &world,
            &mut framebuffer,
            solar_game.as_ref(),
            capture
                .as_ref()
                .and_then(|c| c.body_override())
                .or_else(|| pursuit.as_ref().and_then(|p| p.body_override()))
                .or_else(|| auto.as_ref().and_then(|a| a.body_override()))
                .as_ref(),
        )?
        .stats;

        // El mismo compositor que headless: la ventana y `P` muestran el cuadro final.
        let lines = show_overlay.then(|| {
            overlay_lines(
                Some(fps),
                &stats,
                &overlay_state(&world),
                world.ship.speed,
                world.debug_oxygen01,
                world.time,
            )
        });
        compose_presentation_with_hud(
            &mut framebuffer,
            &world,
            &PresentationLayers {
                overlay: lines.as_deref(),
                comparison,
                distant_turn,
                horizontal,
                preset: last_preset,
                gameplay_feedback: options.gameplay_feedback,
                perception,
                ending: ending_seconds,
            },
            options.oxygen_hud.enabled,
        )?;
        if io.pressed(Key::P) {
            if let Some(guard) = &output_guard {
                guard.preflight(&options.output)?;
            }
            if kind == TentacleRoute::Auto {
                save_png_exclusive(&options.output, &framebuffer)?;
            } else {
                save_png(&options.output, &framebuffer)?;
            }
            println!("PNG: {}", options.output.display());
        }
        // FPS suavizado para el overlay: media movil exponencial sobre 1/dt.
        fps += (1.0 / dt.max(1e-6) - fps) * (1.0 - (-3.0 * dt).exp());
        title_clock += dt;
        title_frames += 1;
        if title_clock >= 0.5 {
            // Sol con ojo: su estado (O2, G, H, vista) va en el titulo, no sobre la imagen.
            let solar_status = solar_game
                .as_ref()
                .map_or(String::new(), |game| format!(" | {}", game.title(&world)));
            let perception_status = oxygen
                .as_ref()
                .map_or(String::new(), |o| format!(" | {}", perception_title(o)));
            let capture_status = capture
                .as_ref()
                .map(|c| c.title())
                .or_else(|| pursuit.as_ref().map(|p| p.title()))
                .or_else(|| auto.as_ref().map(|a| a.title()))
                .map_or(String::new(), |t| format!(" | {t}{capture_note}"));
            io.set_title(&format!(
                "UMBRAL | {:.0} FPS | v {:.0} u/s | O2 real {:.0}% (tanque {:.0} s, L) | culling {} | LOD {} | tri {} desc {} ras {} | frag {}{solar_status}{perception_status}{capture_status}",
                f64::from(title_frames) / title_clock,
                world.ship.speed,
                world.gameplay.oxygen01 * 100.0,
                oxygen_tank_seconds(&world),
                if world.toggles.frustum_culling {
                    "ON"
                } else {
                    "OFF"
                },
                if world.toggles.lod { "ON" } else { "OFF" },
                stats.triangles_submitted,
                stats.triangles_culled,
                stats.triangles_rasterized,
                stats.fragments_evaluated
            ));
            (title_clock, title_frames) = (0.0, 0);
        }
        let auto_status = auto.as_ref().map(|a| a.status());
        io.present(&LoopFrame {
            framebuffer: &framebuffer,
            world: &world,
            stats: &stats,
            elapsed,
            auto: auto_status.as_ref(),
            auto_frame: auto_frame.as_ref(),
            contact_error_obj: auto
                .as_ref()
                .and_then(|a| a.final_mesh_contact_error(&world)),
        })?;
        frames += 1;
        if options.frame_limit.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    if replay.is_some() && io.is_open() {
        return Err("replay incompleto: no alcanzo Done ni termino metrics.json".into());
    }
    println!("{}", io.summary(frames));
    Ok(())
}
