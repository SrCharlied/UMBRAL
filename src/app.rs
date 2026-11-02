//! Aplicacion del juego: bucle de ventana, entrada y modo headless.

use crate::{
    assets::{obj::load_model, ship_model_path},
    debug::overlay::{OverlayState, draw_overlay, overlay_lines},
    renderer::{framebuffer::Framebuffer, image::save_png, stats::RenderStats},
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
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
        lucid_game::set_lucid_surface,
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
        void_game::set_void_eye,
        watching_stars_game::render_game_frame,
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
pub fn run_game(options: GameOptions) -> Result<(), String> {
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
    options: GameOptions,
    route: PerceptionRoute,
) -> Result<(), String> {
    run_with_perception(options, route, false)
}

/// Estrellas observadoras aprobadas (UMB-310, preview02) sobre el mismo bucle
/// (`--watching-stars`, que implica `--lucid-perception`): la misma escena `route` y el mismo O2
/// manual progresivo que la percepcion; en cada cuadro (inicial, headless, ventana y `P`) los ojos
/// van sobre la escena ya dibujada, antes de overlays y paneles, y la percepcion sigue siendo el
/// ultimo paso de color. Los ojos usan el O2 MOSTRADO, la camara real y la nave real. Valida antes
/// de cualquier IO lo mismo que `run_game_with_lucid_perception`.
pub fn run_game_with_watching_stars(
    options: GameOptions,
    route: PerceptionRoute,
) -> Result<(), String> {
    run_with_perception(options, route, true)
}

/// Guards comunes de las dos entradas con percepcion (antes de cualquier IO) y su `Route`.
fn run_with_perception(
    options: GameOptions,
    route: PerceptionRoute,
    watching: bool,
) -> Result<(), String> {
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
pub fn run_game_with_lucid_surface(options: GameOptions) -> Result<(), String> {
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
pub fn run_game_with_eye_comparison(options: GameOptions) -> Result<(), String> {
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
pub fn run_game_with_distant_turn(options: GameOptions) -> Result<(), String> {
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
pub fn run_game_with_horizontal_follow(options: GameOptions) -> Result<(), String> {
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
pub fn run_game_with_solar_reveal(options: GameOptions) -> Result<(), String> {
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
pub fn run_game_with_solar_and_lucid_surface(options: GameOptions) -> Result<(), String> {
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
    options: GameOptions,
    lucid: bool,
    solar: bool,
) -> Result<(), String> {
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
    let mut next = *oxygen;
    next.press(keys)?;
    let shown = next.advance(elapsed)?;
    *oxygen = next;
    world.debug_oxygen01 = shown;
    advance_frame(world, input, elapsed);
    Ok(())
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

fn run(options: GameOptions, route: Route) -> Result<(), String> {
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
    world.debug_oxygen01 = options.debug_oxygen;
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
    // Objetivo manual y O2 mostrado: arrancan EXACTAMENTE en --debug-oxygen (sin presimular).
    let mut oxygen = if perception {
        Some(OxygenTransition::new(options.debug_oxygen)?)
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
    let mut last_preset = CreaturePreset::Far;
    // El modo solar dibuja siempre con su cuadro (inicial, headless y cada cuadro del bucle); con
    // `--watching-stars` el mismo helper pone los ojos sobre la escena, antes de overlays y post.
    let render = |world: &World, framebuffer: &mut Framebuffer, solar: Option<&SolarGame>| {
        render_game_frame(world, framebuffer, solar, watching)
    };
    let frame = render(&world, &mut framebuffer, solar_game.as_ref())?;
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
        if show_overlay {
            let lines = overlay_lines(
                None,
                &stats,
                &overlay_state(&world),
                world.ship.speed,
                world.debug_oxygen01,
                world.time,
            );
            draw_overlay(&mut framebuffer, &lines);
        }
        if comparison {
            draw_comparison(&mut framebuffer, &world);
        }
        if distant_turn {
            draw_distant_turn(&mut framebuffer, &world, last_preset);
        }
        if horizontal {
            draw_horizontal_follow(&mut framebuffer, &world, last_preset);
        }
        if perception {
            // Ultimo paso de color, tras overlay y paneles: con O2 0 no queda texto brillante.
            apply_lucid_perception(&mut framebuffer, &world)?;
        }
        save_png(&options.output, &framebuffer)?;
        println!("PNG: {} ({width}x{height})", options.output.display());
        return Ok(());
    }

    let mut window = Window::new("UMBRAL", width, height, WindowOptions::default())
        .map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    let mut fps = 60.0;
    let (mut frames, mut last, mut title_clock, mut title_frames) =
        (0usize, Instant::now(), 0.0, 0u32);
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let elapsed = now.duration_since(last).as_secs_f64();
        // Limitar dt evita saltos enormes tras arrastrar la ventana o pausar el proceso.
        let dt = elapsed.min(MAX_PHYSICS_DT);
        last = now;

        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            if let Some(game) = &mut solar_game {
                // Sol con ojo: R vuelve a la vista FRONTAL (conserva O2, G, H y el reloj).
                let reset = SolarKeys {
                    reset: true,
                    ..SolarKeys::default()
                };
                game.apply(&mut world, reset)?;
            } else if distant_turn || horizontal {
                // Candidatas: R repite el ultimo preset (conserva O2, objetivo y H).
                world.place_creature_preset(last_preset)?;
            } else if near_creature {
                // Con --near-creature, R vuelve al arranque frente a la criatura.
                world.place_near_creature()?;
            } else {
                world.reset_ship();
            }
        }
        if window.is_key_pressed(Key::F3, KeyRepeat::No) {
            world.toggles.frustum_culling ^= true;
        }
        if window.is_key_pressed(Key::F4, KeyRepeat::No) {
            world.toggles.lod ^= true;
        }
        if window.is_key_pressed(Key::F1, KeyRepeat::No) {
            show_overlay ^= true;
        }
        if window.is_key_pressed(Key::F6, KeyRepeat::No) {
            world.toggles.backface_culling ^= true;
        }
        if window.is_key_pressed(Key::F5, KeyRepeat::No) {
            world.toggles.freeze_orbits ^= true;
        }
        let mut preset = None;
        for (key, value) in [
            (Key::Key1, 1.0),
            (Key::Key2, 0.5),
            (Key::Key3, 0.25),
            (Key::Key4, 0.05),
        ] {
            if window.is_key_pressed(key, KeyRepeat::No) {
                preset = Some(value);
            }
        }
        // Una pulsacion por paso, sin repeticion al mantener la tecla. Los presets 1-4
        // siguen disponibles; M/N se aplican despues si coinciden en el mismo cuadro.
        let oxygen_keys = OxygenKeys {
            preset,
            increase: window.is_key_pressed(Key::M, KeyRepeat::No),
            decrease: window.is_key_pressed(Key::N, KeyRepeat::No),
        };
        if oxygen.is_none() {
            // Sin --lucid-perception: el O2 salta como siempre.
            if let Some(value) = oxygen_keys.preset {
                world.debug_oxygen01 = value;
            }
            world.debug_oxygen01 = step_debug_oxygen(
                world.debug_oxygen01,
                oxygen_keys.increase,
                oxygen_keys.decrease,
            );
        }
        if distant_turn || horizontal {
            for (key, preset) in [
                (Key::Key7, CreaturePreset::Near),
                (Key::Key8, CreaturePreset::Far),
            ] {
                if window.is_key_pressed(key, KeyRepeat::No) {
                    last_preset = preset;
                    world.place_creature_preset(preset)?;
                }
            }
            if window.is_key_pressed(Key::H, KeyRepeat::No) {
                // Solo cambia la eleccion: apagado vuelve al reposo/frente con su transicion.
                if horizontal {
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
        if let Some(game) = &mut solar_game {
            let keys = SolarKeys {
                frontal: window.is_key_pressed(Key::Key7, KeyRepeat::No),
                oblique: window.is_key_pressed(Key::Key8, KeyRepeat::No),
                follow: window.is_key_pressed(Key::H, KeyRepeat::No),
                target: window.is_key_pressed(Key::G, KeyRepeat::No),
                reset: false,
            };
            game.apply(&mut world, keys)?;
        }
        if comparison && window.is_key_pressed(Key::G, KeyRepeat::No) {
            // Solo cambia la eleccion: la mirada llega con su transicion de siempre.
            let next = match world.creature_eye_target() {
                Some(EyeTarget::Camera) => EyeTarget::Ship,
                _ => EyeTarget::Camera,
            };
            world.set_creature_eye_target(next)?;
        }
        // Fisica con `dt` limitado; revelacion y mirada con los segundos reales, hacia la nave
        // ya movida (F5 no las congela). Con las candidatas avanza antes el giro global o el
        // seguimiento y el ojo los compensa.
        // Con --lucid-perception las teclas solo cambian el objetivo y el O2 mostrado avanza aqui
        // con los mismos segundos reales (sin el limite de la fisica y sin que F5 lo congele).
        match &mut oxygen {
            Some(oxygen) => advance_perception_frame(
                &mut world,
                oxygen,
                oxygen_keys,
                &keyboard_input(&window),
                elapsed,
            )?,
            None => advance_frame(&mut world, &keyboard_input(&window), elapsed),
        }
        let stats = render(&world, &mut framebuffer, solar_game.as_ref())?.stats;

        if show_overlay {
            let lines = overlay_lines(
                Some(fps),
                &stats,
                &overlay_state(&world),
                world.ship.speed,
                world.debug_oxygen01,
                world.time,
            );
            draw_overlay(&mut framebuffer, &lines);
        }
        if comparison {
            draw_comparison(&mut framebuffer, &world);
        }
        if distant_turn {
            draw_distant_turn(&mut framebuffer, &world, last_preset);
        }
        if horizontal {
            draw_horizontal_follow(&mut framebuffer, &world, last_preset);
        }
        if perception {
            // El mismo ultimo paso que headless: la ventana y `P` muestran el cuadro final.
            apply_lucid_perception(&mut framebuffer, &world)?;
        }
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            save_png(&options.output, &framebuffer)?;
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
            window.set_title(&format!(
                "UMBRAL | {:.0} FPS | v {:.0} u/s | culling {} | LOD {} | tri {} desc {} ras {} | frag {}{solar_status}{perception_status}",
                f64::from(title_frames) / title_clock,
                world.ship.speed,
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
        // minifb presenta pixeles ya calculados; no dibuja los triangulos de la escena.
        window
            .update_with_buffer(&framebuffer.pixels, width, height)
            .map_err(|e| e.to_string())?;
        frames += 1;
        if options.frame_limit.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    println!("Ventana: {frames} frames mostrados");
    Ok(())
}
