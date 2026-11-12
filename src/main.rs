use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::path::PathBuf;
use umbral::{
    app::{
        GameOptions, PerceptionRoute, run_game, run_game_with_distant_turn,
        run_game_with_eye_comparison, run_game_with_horizontal_follow,
        run_game_with_lucid_perception, run_game_with_lucid_surface,
        run_game_with_solar_and_lucid_surface, run_game_with_solar_reveal,
        run_game_with_tentacle_auto_capture, run_game_with_tentacle_capture,
        run_game_with_tentacle_live_pursuit, run_game_with_tentacle_spatial_pursuit,
        run_game_with_void_eye, run_game_with_watching_stars,
    },
    assets::{obj::load_model, ship_model_path},
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        image::save_png,
        legacy::{BACKGROUND, render_model},
    },
    visual::{
        creature_distant_turn::validate_distant_turn_flags,
        creature_eye_comparison::validate_eye_compare_flags,
        creature_eye_follow::validate_eye_follow_flags,
        creature_horizontal_follow::validate_horizontal_follow_flags,
        creature_reveal::{parse_debug_oxygen, validate_reveal_flags},
        creature_viewer::validate_output_path,
        lucid_game::{LucidSurfaceRequest, validate_lucid_surface_flags},
        perception_game::validate_lucid_perception_flags,
        solar_game::{SolarRevealRequest, validate_solar_reveal_flags},
        staged_creature_reveal::validate_staged_flags,
        static_creature::CreatureOptions,
        tentacle_auto_capture::validate_auto_capture_flags,
        tentacle_live_pursuit::validate_live_pursuit_flags,
        tentacle_solar_game::{TentacleCaptureFlags, validate_tentacle_capture_flags},
        tentacle_spatial_pursuit::validate_spatial_pursuit_flags,
        void_game::{VoidEyeRequest, validate_void_eye_flags},
        watching_stars_game::validate_watching_stars_flags,
    },
};

/// Duracion del tanque de oxigeno con la experiencia completa por defecto (ventana sin
/// `--classic` ni `--oxygen-seconds`): ojos hacia los 42 s, percepcion a los 60 s y muerte a
/// los 120 s. `L` sigue alternando 420/120/60/30 s.
const DEFAULT_GAME_OXYGEN_SECONDS: f64 = 120.0;

fn positive_argument(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<usize, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("Falta la cantidad despues de {flag}"))?
        .parse::<usize>()
        .map_err(|_| format!("{flag} necesita un entero positivo"))?;
    if value == 0 {
        return Err(format!("{flag} debe ser mayor que cero"));
    }
    Ok(value)
}

fn run() -> Result<(), String> {
    let mut headless = false;
    let mut frame_limit = None;
    let (mut width, mut height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut output = PathBuf::from("renders/nave.png");
    let (mut legacy, mut time, mut near, mut culling) = (false, 0.0, None, true);
    let (mut lod, mut overlay) = (true, false);
    let mut gameplay_feedback = false;
    let (mut creature_model, mut creature_texture, mut near_creature) = (None, None, false);
    let (mut creature_reveal, mut debug_oxygen) = (false, None);
    let (mut creature_reveal_stages, mut creature_eye_follow) = (false, false);
    let (mut creature_eye_compare, mut creature_distant_turn) = (false, false);
    let (mut creature_horizontal_follow, mut creature_solar_reveal) = (false, false);
    let mut lucid_surface = false;
    let mut void_eye = false;
    let mut lucid_perception = false;
    let mut watching_stars = false;
    let mut oxygen_seconds: Option<f64> = None;
    let mut classic = false;
    let mut capture_flags = TentacleCaptureFlags::default();
    let mut live_pursuit = false;
    let mut spatial_pursuit = false;
    let mut auto_capture = false;
    let mut auto_replay = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--headless" => headless = true,
            "--output" => {
                output = PathBuf::from(args.next().ok_or("Falta la ruta despues de --output")?)
            }
            "--legacy" => legacy = true,
            "--no-culling" => culling = false,
            "--no-lod" => lod = false,
            "--overlay" => overlay = true,
            "--gameplay-feedback" => gameplay_feedback = true,
            "--time" => {
                time = args
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|t| t.is_finite() && *t >= 0.0)
                    .ok_or("--time necesita segundos >= 0")?
            }
            "--near" => {
                near = Some(
                    args.next()
                        .and_then(|v| v.parse::<usize>().ok())
                        .ok_or("--near necesita el indice de un cuerpo (0 = Sol)")?,
                )
            }
            "--creature-model" => {
                creature_model = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la ruta despues de --creature-model")?,
                ))
            }
            "--creature-texture" => {
                creature_texture = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la ruta despues de --creature-texture")?,
                ))
            }
            "--near-creature" => near_creature = true,
            "--creature-reveal" => creature_reveal = true,
            "--creature-reveal-stages" => creature_reveal_stages = true,
            "--creature-eye-follow" => creature_eye_follow = true,
            "--creature-eye-compare" => creature_eye_compare = true,
            "--creature-distant-turn" => creature_distant_turn = true,
            "--creature-horizontal-follow" => creature_horizontal_follow = true,
            "--creature-solar-reveal" => creature_solar_reveal = true,
            "--lucid-surface" => lucid_surface = true,
            "--void-eye" => void_eye = true,
            "--lucid-perception" => lucid_perception = true,
            "--watching-stars" => watching_stars = true,
            "--classic" => classic = true,
            "--tentacle-capture-debug" => capture_flags.enabled = true,
            "--tentacle-live-pursuit" => live_pursuit = true,
            "--tentacle-spatial-pursuit" => spatial_pursuit = true,
            "--tentacle-auto-capture" => auto_capture = true,
            "--auto-capture-replay" => {
                auto_replay = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la carpeta despues de --auto-capture-replay")?,
                ))
            }
            "--tentacle-capture-package" => {
                capture_flags.capture_dir = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la carpeta despues de --tentacle-capture-package")?,
                ))
            }
            "--tentacle-growth-package" => {
                capture_flags.growth_package = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la ruta despues de --tentacle-growth-package")?,
                ))
            }
            "--tentacle-arm-correspondence" => {
                capture_flags.correspondence = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la ruta despues de --tentacle-arm-correspondence")?,
                ))
            }
            "--tentacle-capture-time" => {
                capture_flags.start_time = Some(
                    args.next()
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|t| t.is_finite() && *t >= 0.0)
                        .ok_or("--tentacle-capture-time necesita segundos finitos >= 0")?,
                )
            }
            "--debug-oxygen" => {
                debug_oxygen = Some(parse_debug_oxygen(
                    &args
                        .next()
                        .ok_or("Falta el valor despues de --debug-oxygen")?,
                )?)
            }
            "--oxygen-seconds" => {
                oxygen_seconds = Some(
                    args.next()
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|s| s.is_finite() && *s > 0.0)
                        .ok_or("--oxygen-seconds necesita segundos finitos > 0")?,
                )
            }
            "--frames" => frame_limit = Some(positive_argument(&mut args, "--frames")?),
            "--width" => width = positive_argument(&mut args, "--width")?,
            "--height" => height = positive_argument(&mut args, "--height")?,
            "--help" | "-h" => {
                println!(
                    "cargo run --release -- [--headless] [--output archivo.png] [--frames N] [--width N] [--height N] [--oxygen-seconds S] [--classic]\nPor defecto, en ventana: estrellas observadoras + percepcion + superficie lucida + ojo del vacio y tanque de 120 s. --classic (o --headless) usa el juego sin esos efectos y el tanque de 420 s\nOxigeno: --oxygen-seconds S fija cuantos segundos dura el tanque completo (por defecto 420); en juego, L alterna 420/120/60/30 s\nResolucion por defecto: {DEFAULT_WIDTH}x{DEFAULT_HEIGHT} (maximo {MAX_PIXELS} pixeles)\nA/D o flechas: rotar | R: restablecer | P: guardar PNG | Esc: salir\nCriatura estatica (debug, opt-in): --creature-model modelo.obj --creature-texture textura.png (siempre juntos) [--near-creature: arrancar frente a ella; R vuelve ahi]. Los flags de criatura son incompatibles con --legacy; solo el arranque --near-creature es incompatible con --near. Con criatura, --output debe ser .png y distinto de las entradas\nRevelacion por oxigeno (debug, opt-in): --creature-reveal (requiere la criatura; no mueve la nave) | --debug-oxygen X: oxigeno de depuracion inicial en [0, 1] (por defecto 1); teclas 1-4: 100/50/25/5 %\nRevelacion por etapas (debug, opt-in): --creature-reveal-stages (requiere la criatura; incompatible con --creature-reveal; no mueve la nave): primero el ojo y despues el cuerpo\nMirada local (debug, opt-in): --creature-eye-follow (requiere --creature-reveal-stages): el ojo mira a la nave dentro de sus limites; cuerpo y tentaculos quietos\nComparacion de la mirada (debug, opt-in): --creature-eye-compare (requiere --creature-eye-follow): arranca mirando a la camara; G alterna NAVE/CAMARA; panel inferior\nGiro global lejano (candidata, debug, opt-in): --creature-distant-turn (requiere --creature-eye-compare): de lejos la criatura entera gira lenta y limitada (20/12 grados, t95 8 s) hacia el punto de G; de cerca solo el ojo. Arranca LEJOS; H giro ON/OFF, 7 CERCA, 8 LEJOS, R repite el preset\nSeguimiento horizontal (candidata 02, debug, opt-in): --creature-horizontal-follow (requiere --creature-eye-compare; no con --creature-distant-turn): cerca y lejos el cuerpo entero gira solo en horizontal, lento (t95 4 s, tope 18 grados/s), hacia el punto de G en cualquier lado. Arranca LEJOS; H seguimiento ON/OFF (OFF vuelve al frente), 7 CERCA, 8 LEJOS, R repite el preset\nSol con ojo (composicion aprobada 03, opt-in): --creature-solar-reveal (requiere --creature-model y --creature-texture; implica etapas, mirada, comparacion y seguimiento horizontal 02, que pueden repetirse; no con --legacy, --creature-reveal, --near, --near-creature ni --creature-distant-turn): la criatura en el centro del Sol (radio 80, yaw 0), mirada a la camara y luz de camara; arranca FRONTAL. 7 FRONTAL, 8 OBLICUA, R vuelve a FRONTAL, H seguimiento ON/OFF, G CAMARA/NAVE, 1-4 y M/N el O2 (titulo de la ventana)
Superficie lucida (aprobada 02, opt-in): --lucid-surface: el planeta Antena existente usa la superficie lucida (grietas rojo-ambar bajo 50 % de O2; con O2 >= 50 % el cuadro es el de siempre); mismo material, radio, orbita y rotacion. Con el juego normal (--near 1 arranca junto a Antena; admite la criatura estatica y su revelacion) o con --creature-solar-reveal; no con --legacy ni, fuera del Sol con ojo, con --creature-eye-compare, --creature-distant-turn o --creature-horizontal-follow. 1-4 y M/N cambian el O2
Ojo del vacio (aprobado 02, opt-in): --void-eye: el planeta Vigilia existente usa el ojo del vacio (pupila negra que mira siempre a la NAVE y una corona rojo-ambar que respira con el reloj de simulacion; F5 la congela); mismo material, radio, orbita y rotacion; no depende del O2. Con el juego normal (--near 3 arranca junto a Vigilia; admite la criatura estatica y su revelacion) o con --creature-solar-reveal, y en ambos con o sin --lucid-surface; no con --legacy ni, fuera del Sol con ojo, con --creature-eye-compare, --creature-distant-turn o --creature-horizontal-follow
Percepcion alterada (aprobada 02, opt-in): --lucid-perception: ultimo paso de color de cada cuadro (grises + morado del monstruo, tunel hacia la nave proyectada y vision borrosa bajo 50 % de O2; negro total solo con O2 0; con O2 >= 50 % el cuadro es el de siempre). Con el juego normal o con --creature-solar-reveal, y en ambos con o sin --lucid-surface y --void-eye; no con --legacy ni, fuera del Sol con ojo, con --creature-eye-compare, --creature-distant-turn o --creature-horizontal-follow; --output debe ser .png. Progresivo: 1-4 y M/N cambian el OBJETIVO de O2 y el O2 mostrado lo sigue a 0.25 por segundo real, al bajar y al recuperar (F5 no lo congela; R lo conserva; nada consume O2 solo); el titulo muestra O2 y objetivo
Estrellas observadoras (aprobadas 02, opt-in): --watching-stars: implica --lucid-perception (el flag explicito es redundante) y sus mismas escenas y combinaciones: con el juego normal o con --creature-solar-reveal, con o sin --lucid-surface y --void-eye; no con --legacy ni, fuera del Sol con ojo, con --creature-eye-compare, --creature-distant-turn o --creature-horizontal-follow; --output debe ser .png. Las 700 estrellas del cielo abren ojos pequenos segun el O2 MOSTRADO (ninguna con O2 >= 65 %) que miran a la nave proyectada con la camara real; van sobre la escena, antes del overlay y de la percepcion. Mismos controles: 1-4 y M/N el objetivo de O2, F5 no congela la transicion, R la conserva
Composicion OBSERVADORA del tentaculo (debug, opt-in; NO es captura ni agarre): --tentacle-capture-debug (requiere --creature-solar-reveal) --tentacle-capture-package <capture_v2> --tentacle-growth-package <growth_package.txt> --tentacle-arm-correspondence <arm_correspondence.txt> [--tentacle-capture-time S: activa al arrancar con reloj S, replay determinista]. F7 toma la POSICION de la nave y el transform actual de la criatura y reproduce el brazo aprobado en la capa del cuerpo (misma alfa: con O2 >= 50 % sigue invisible); otra F7 cancela. No mueve nave, camara, input ni O2
PERSECUCION VIVA del tentaculo (prototipo opt-in; NO es captura ni agarre): --tentacle-live-pursuit (requiere --creature-solar-reveal) con los mismos --tentacle-capture-package, --tentacle-growth-package y --tentacle-arm-correspondence. Con O2 <= 15 % la punta persigue la POSICION ACTUAL de la nave con rapidez limitada (15 % lenta, 10 % mas rapida, 5 % y 0 % rapida; valores candidatos) y raiz fija al cuerpo; con O2 > 15 % vuelve al reposo. Misma capa y alfa del cuerpo. No se combina con --tentacle-capture-debug ni --tentacle-capture-time. No mueve nave, camara, input ni O2; sin muerte, agarre ni arrastre
PERSECUCION ESPACIAL 3D del tentaculo (prototipo opt-in; NO es captura ni agarre): --tentacle-spatial-pursuit (requiere --creature-solar-reveal) con los mismos --tentacle-capture-package, --tentacle-growth-package y --tentacle-arm-correspondence. Mismo controlador y O2 que la persecucion viva; deformacion 3D continua (codo espacial con memoria de lado) tambien por detras de la criatura; bola minima mayor (dominio exacto declarado). No se combina con --tentacle-live-pursuit, --tentacle-capture-debug ni --tentacle-capture-time. No mueve nave, camara, input ni O2; sin muerte, agarre ni arrastre
CAPTURA AUTOMATICA por contacto (GT3, opt-in): --tentacle-auto-capture (requiere --creature-solar-reveal y --lucid-perception o --watching-stars) con los mismos --tentacle-capture-package, --tentacle-growth-package y --tentacle-arm-correspondence. Persecucion espacial existente con O2 <= 15 %; si la punta ALCANZA la nave, la nave queda atrapada en ese cuadro donde esta (sin T ni quietud): vuelo y R/7/8 bloqueados, gesto aprobado B1/P1, arrastre y retencion aprobados; O2 > 15 % la suelta sin inercia; X es un escape de diagnostico. No se combina con las otras rutas del tentaculo. [--auto-capture-replay <carpeta VACIA>: replay determinista del mismo bucle sin ventana, PNG y metrics.json]"
                );
                return Ok(());
            }
            _ => return Err(format!("Argumento desconocido: {argument}. Usa --help.")),
        }
    }
    if auto_replay.is_some() && frame_limit.is_some() {
        return Err(
            "--auto-capture-replay no se combina con --frames: requiere completar Done".into(),
        );
    }
    // Experiencia completa por defecto en la VENTANA (`cargo run` sin flags): ojos en las
    // estrellas (implican la percepcion), superficie lucida en Antena, ojo del vacio en Vigilia
    // y un tanque de oxigeno de demostracion. No aplica con `--headless` (las capturas y pruebas
    // conservan el cuadro de siempre), con `--classic`, con `--legacy` ni con las candidatas de
    // mirada que no admiten estos efectos fuera del Sol con ojo.
    let default_effects = !classic
        && !headless
        && !legacy
        && (creature_solar_reveal
            || !(creature_eye_compare || creature_distant_turn || creature_horizontal_follow));
    if default_effects {
        watching_stars = true;
        lucid_surface = true;
        void_eye = true;
        oxygen_seconds.get_or_insert(DEFAULT_GAME_OXYGEN_SECONDS);
    }
    // Composicion observadora del tentaculo (opt-in): sus combinaciones antes de cualquier archivo.
    capture_flags.solar = creature_solar_reveal;
    capture_flags.legacy = legacy;
    // Persecucion viva (opt-in): con su flag decide ella (mismas rutas, sin F7 ni reloj); sin el
    // flag las reglas de la composicion observadora no cambian.
    // Persecucion espacial (opt-in): con su flag decide ella (incompatible con la viva); sin el flag
    // las reglas de la persecucion viva y de la composicion no cambian.
    // Captura automatica (GT3, opt-in): con su flag decide ella (persecucion espacial + gesto + arrastre);
    // sin el flag las reglas de las rutas del tentaculo no cambian.
    let auto_request = validate_auto_capture_flags(
        auto_capture,
        auto_replay.is_some(),
        spatial_pursuit,
        live_pursuit,
        &capture_flags,
    )?;
    let spatial_request = if auto_request.is_some() {
        None
    } else {
        validate_spatial_pursuit_flags(spatial_pursuit, live_pursuit, &capture_flags)?
    };
    let live_request = if spatial_request.is_some() || auto_request.is_some() {
        None
    } else {
        validate_live_pursuit_flags(live_pursuit, &capture_flags)?
    };
    let capture_request =
        if live_request.is_some() || spatial_request.is_some() || auto_request.is_some() {
            None
        } else {
            validate_tentacle_capture_flags(&capture_flags)?
        };
    // Antes de expandir los flags implicitos del Sol con ojo y de cualquier archivo.
    validate_lucid_surface_flags(
        lucid_surface,
        LucidSurfaceRequest {
            legacy,
            solar: creature_solar_reveal,
            eye_compare: creature_eye_compare,
            distant_turn: creature_distant_turn,
            horizontal_follow: creature_horizontal_follow,
        },
    )?;
    validate_void_eye_flags(
        void_eye,
        VoidEyeRequest {
            legacy,
            solar: creature_solar_reveal,
            eye_compare: creature_eye_compare,
            distant_turn: creature_distant_turn,
            horizontal_follow: creature_horizontal_follow,
        },
    )?;
    validate_lucid_perception_flags(
        lucid_perception,
        LucidSurfaceRequest {
            legacy,
            solar: creature_solar_reveal,
            eye_compare: creature_eye_compare,
            distant_turn: creature_distant_turn,
            horizontal_follow: creature_horizontal_follow,
        },
    )?;
    validate_watching_stars_flags(
        watching_stars,
        LucidSurfaceRequest {
            legacy,
            solar: creature_solar_reveal,
            eye_compare: creature_eye_compare,
            distant_turn: creature_distant_turn,
            horizontal_follow: creature_horizontal_follow,
        },
    )?;
    let creature = CreatureOptions::from_flags(
        creature_model,
        creature_texture,
        near_creature,
        near,
        legacy,
    )?;
    // Antes que los demas: el flag solar implica etapas, mirada, comparacion y seguimiento
    // horizontal (redundantes si se pasan) y rechaza las rutas que cambian su composicion.
    validate_solar_reveal_flags(
        creature_solar_reveal,
        SolarRevealRequest {
            has_creature: creature.is_some(),
            legacy,
            near_body: near.is_some(),
            near_creature,
            uniform_reveal: creature_reveal,
            distant_turn: creature_distant_turn,
        },
    )?;
    if creature_solar_reveal {
        creature_reveal_stages = true;
        creature_eye_follow = true;
        creature_eye_compare = true;
        creature_horizontal_follow = true;
    }
    validate_reveal_flags(
        creature_reveal,
        creature.is_some(),
        debug_oxygen.is_some(),
        legacy,
    )?;
    validate_staged_flags(
        creature_reveal_stages,
        creature.is_some(),
        creature_reveal,
        legacy,
    )?;
    validate_eye_follow_flags(
        creature_eye_follow,
        creature_reveal_stages,
        creature.is_some(),
        legacy,
    )?;
    validate_eye_compare_flags(creature_eye_compare, creature_eye_follow, legacy)?;
    validate_distant_turn_flags(creature_distant_turn, creature_eye_compare, legacy)?;
    validate_horizontal_follow_flags(
        creature_horizontal_follow,
        creature_eye_compare,
        creature_distant_turn,
        legacy,
    )?;
    if let Some(creature) = &creature {
        validate_output_path(&output, &[&creature.model, &creature.texture])?;
    }
    if !legacy {
        let options = GameOptions {
            width,
            height,
            headless,
            frame_limit,
            output,
            time,
            near,
            culling,
            lod,
            overlay,
            creature,
            creature_reveal,
            creature_reveal_stages,
            creature_eye_follow,
            debug_oxygen: debug_oxygen.unwrap_or(1.0),
        };
        let options = umbral::app::ConfiguredGameOptions {
            options,
            gameplay_feedback,
            oxygen_seconds,
            oxygen_source: if debug_oxygen.is_some() {
                umbral::app::OxygenSource::Debug
            } else {
                umbral::app::OxygenSource::Real
            },
        };
        let route = PerceptionRoute {
            lucid: lucid_surface,
            void: void_eye,
            solar: creature_solar_reveal,
        };
        if let Some(request) = auto_request {
            // Captura automatica por contacto (GT3) sobre la misma ruta solar pedida.
            return run_game_with_tentacle_auto_capture(
                options,
                route,
                lucid_perception || watching_stars,
                watching_stars,
                request,
                auto_replay,
            );
        }
        if let Some(request) = spatial_request {
            // Persecucion espacial 3D (prototipo, NO agarre) sobre la misma ruta solar pedida.
            return run_game_with_tentacle_spatial_pursuit(
                options,
                route,
                lucid_perception || watching_stars,
                watching_stars,
                request,
            );
        }
        if let Some(request) = live_request {
            // Persecucion viva (prototipo, NO agarre) sobre la misma ruta solar pedida.
            return run_game_with_tentacle_live_pursuit(
                options,
                route,
                lucid_perception || watching_stars,
                watching_stars,
                request,
            );
        }
        if let Some(request) = capture_request {
            // Composicion observadora (debug, NO agarre) sobre la misma ruta solar pedida.
            return run_game_with_tentacle_capture(
                options,
                route,
                lucid_perception || watching_stars,
                watching_stars,
                request,
            );
        }
        if watching_stars {
            // Implica --lucid-perception (redundante si se pasa): misma escena y mismo O2 manual.
            return run_game_with_watching_stars(options, route);
        }
        if lucid_perception {
            return run_game_with_lucid_perception(options, route);
        }
        if void_eye {
            return run_game_with_void_eye(options, lucid_surface, creature_solar_reveal);
        }
        let game = if creature_solar_reveal && lucid_surface {
            run_game_with_solar_and_lucid_surface
        } else if creature_solar_reveal {
            run_game_with_solar_reveal
        } else if creature_horizontal_follow {
            run_game_with_horizontal_follow
        } else if creature_distant_turn {
            run_game_with_distant_turn
        } else if creature_eye_compare {
            run_game_with_eye_comparison
        } else if lucid_surface {
            run_game_with_lucid_surface
        } else {
            run_game
        };
        return game(options);
    }
    let model = load_model(&ship_model_path())?;
    let mut framebuffer = Framebuffer::new(width, height, BACKGROUND)
        .map_err(|e| format!("{e}: {width}x{height} (maximo {MAX_PIXELS} pixeles)"))?;
    let mut angle = 0.0;
    let stats = render_model(&mut framebuffer, &model, angle)?;
    println!(
        "Modelo: {} vertices, {} caras, {} materiales",
        model.vertices.len(),
        model.faces.len(),
        model.materials.len()
    );
    println!(
        "Caras recorridas: {} | Escrituras de pixel: {}",
        stats.faces_processed, stats.pixels_written
    );
    if headless {
        save_png(&output, &framebuffer)?;
        println!("PNG: {} ({width}x{height})", output.display());
        return Ok(());
    }
    let mut window = Window::new(
        "UMBRAL - Carga de modelos OBJ - 24531",
        width,
        height,
        WindowOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    let mut frames = 0;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let previous_angle = angle;
        if window.is_key_down(Key::A) || window.is_key_down(Key::Left) {
            angle -= 0.03;
        }
        if window.is_key_down(Key::D) || window.is_key_down(Key::Right) {
            angle += 0.03;
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            angle = 0.0;
        }
        if angle != previous_angle {
            render_model(&mut framebuffer, &model, angle)?;
        }
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            save_png(&output, &framebuffer)?;
            println!("PNG: {}", output.display());
        }
        // minifb presenta pixeles ya calculados; no dibuja los triangulos del modelo.
        window
            .update_with_buffer(&framebuffer.pixels, width, height)
            .map_err(|e| e.to_string())?;
        frames += 1;
        if frame_limit.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    println!("Ventana: {frames} frames mostrados");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
