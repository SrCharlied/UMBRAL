//! Supervisor tracer: approved VoidEye02 on Vigilia in the actual application.
use std::{fs::File, io::BufReader, process::Command};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::{framebuffer::Framebuffer, shader::ShaderKind},
    scene::world::{SPACE_COLOR, World},
};

#[test]
fn void_flag_renders_vigilia_with_the_approved_shader_in_the_real_game() {
    let (width, height) = (320, 180);
    let path =
        std::env::temp_dir().join(format!("umbral_void_game_first_{}.png", std::process::id()));
    let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--void-eye",
            "--headless",
            "--width",
            "320",
            "--height",
            "180",
            "--near",
            "3",
            "--debug-oxygen",
            "1",
            "--time",
            "1.3",
            "--no-lod",
            "--output",
        ])
        .arg(&path)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "the real game must accept --void-eye: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut world = World::new(
        load_model(&ship_model_path()).unwrap(),
        width as f64 / height as f64,
    )
    .unwrap();
    world.time = 1.3;
    world.debug_oxygen01 = 1.0;
    world.toggles.lod = false;
    assert_eq!(world.system.bodies[3].name, "Vigilia");
    world.place_near(3, 25.0).unwrap();
    let mut baseline = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    world.render(&mut baseline);
    world.system.bodies[3].shader = ShaderKind::VoidEye;
    let mut expected = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    world.render(&mut expected);
    assert!(
        expected.pixels != baseline.pixels,
        "the selected planet must actually show the effect"
    );
    let expected_rgb: Vec<u8> = expected
        .pixels
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect();
    let mut reader = png::Decoder::new(BufReader::new(File::open(&path).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (width as u32, height as u32));
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert!(
        bytes[..info.buffer_size()] == expected_rgb,
        "CLI must apply unchanged VoidEye02 only to Vigilia, with its original material and real World renderer"
    );
    std::fs::remove_file(path).unwrap();
}

// --- Bronya: incrementos RED/GREEN y guardas de la integracion (ver la evidencia). ---

use umbral::{scene::solar_system::SolarSystem, visual::void_game::set_void_eye};

fn new_world(width: usize, height: usize) -> World {
    World::new(
        load_model(&ship_model_path()).unwrap(),
        width as f64 / height as f64,
    )
    .unwrap()
}

/// Todo lo observable del mundo que el ojo del vacio no debe tocar (y el sistema entero).
fn snapshot(world: &World) -> String {
    format!(
        "{:?} | cam {:?} {:?} | nave {:?} {:?} {} | t {} O2 {} | culling {} lod {} back {} freeze {} | luz {:?}",
        world.system,
        world.camera.position,
        world.camera.target,
        world.ship.position,
        world.ship.orientation,
        world.ship.speed,
        world.time,
        world.debug_oxygen01,
        world.toggles.frustum_culling,
        world.toggles.lod,
        world.toggles.backface_culling,
        world.toggles.freeze_orbits,
        world.system.light_position()
    )
}

#[test]
fn vigilia_is_chosen_by_unique_valid_identity_and_errors_leave_the_world_intact() {
    type Edit = fn(&mut SolarSystem);
    let cases: [(&str, Edit, &str); 17] = [
        ("sin Vigilia", |s| s.bodies[3].name = "Otra", "Vigilia"),
        (
            "Vigilia quitada",
            |s| {
                s.bodies.remove(3);
            },
            "Vigilia",
        ),
        (
            "Vigilia duplicada",
            |s| s.bodies[2].name = "Vigilia",
            "ambigu",
        ),
        (
            "Vigilia duplicada al final",
            |s| {
                let copy = s.bodies[3].clone();
                s.bodies.push(copy);
            },
            "ambigu",
        ),
        (
            "Vigilia es un sol",
            |s| s.bodies[3].shader = ShaderKind::AnimatedSun,
            "shader",
        ),
        (
            "Vigilia lucida",
            |s| s.bodies[3].shader = ShaderKind::LucidSurface,
            "shader",
        ),
        (
            "Vigilia plana",
            |s| s.bodies[3].shader = ShaderKind::Flat,
            "shader",
        ),
        (
            "radio no finito",
            |s| s.bodies[3].body_radius = f64::NAN,
            "radio",
        ),
        (
            "radio infinito",
            |s| s.bodies[3].body_radius = f64::INFINITY,
            "radio",
        ),
        ("radio 0", |s| s.bodies[3].body_radius = 0.0, "radio"),
        (
            "radio negativo",
            |s| s.bodies[3].body_radius = -20.0,
            "radio",
        ),
        ("sin orbita", |s| s.bodies[3].orbit.radius = 0.0, "orbita"),
        (
            "orbita negativa",
            |s| s.bodies[3].orbit.radius = -520.0,
            "orbita",
        ),
        (
            "orbita no finita",
            |s| s.bodies[3].orbit.radius = f64::NAN,
            "orbita",
        ),
        (
            "velocidad orbital no finita",
            |s| s.bodies[3].orbit.angular_speed = f64::INFINITY,
            "orbita",
        ),
        (
            "rotacion no finita",
            |s| s.bodies[3].orbit.axial_speed = f64::NAN,
            "orbita",
        ),
        ("fase no finita", |s| s.bodies[3].phase = f64::NAN, "orbita"),
    ];
    for (name, edit, needle) in cases {
        for enabled in [true, false] {
            let mut world = new_world(64, 36);
            edit(&mut world.system);
            let before = snapshot(&world);
            let error = set_void_eye(&mut world, enabled).unwrap_err();
            assert!(error.contains(needle), "{name}: {error}");
            assert_eq!(
                snapshot(&world),
                before,
                "{name}: el Err no debe mutar nada"
            );
        }
    }
    // Reordenado: se elige por identidad (nombre), no por un indice fijo ni memorizado.
    for (a, b) in [(1, 3), (0, 3)] {
        let mut world = new_world(64, 36);
        world.system.bodies.swap(a, b);
        let mut expected = new_world(64, 36);
        expected.system.bodies.swap(a, b);
        expected.system.bodies[a].shader = ShaderKind::VoidEye;
        assert_eq!(set_void_eye(&mut world, true), Ok(a));
        assert_eq!(world.system.bodies[a].name, "Vigilia");
        assert_eq!(snapshot(&world), snapshot(&expected), "solo cambia Vigilia");
    }
    // Sin cuerpos intermedios: el indice sale de la busqueda en cada llamada.
    let mut world = new_world(64, 36);
    world.system.bodies.remove(1);
    assert_eq!(set_void_eye(&mut world, true), Ok(2));
    assert_eq!(world.system.bodies[2].shader, ShaderKind::VoidEye);
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-void-game-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// El binario real, siempre headless (nunca abre ventana).
fn run_cli(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

#[test]
fn the_command_line_rejects_void_combinations_outside_its_routes_before_any_io() {
    let dir = temp_dir("cli-combos");
    let (model, texture, output) = (
        dir.join("no.obj"),
        dir.join("no.png"),
        dir.join("salida.png"),
    );
    let o = std::ffi::OsStr::new;
    let creature = [
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
        o("--output"),
        output.as_os_str(),
        o("--void-eye"),
    ];
    let compare = [
        o("--creature-reveal-stages"),
        o("--creature-eye-follow"),
        o("--creature-eye-compare"),
    ];
    let cases: Vec<(Vec<&std::ffi::OsStr>, &str)> = vec![
        (
            vec![
                o("--legacy"),
                o("--void-eye"),
                o("--output"),
                output.as_os_str(),
            ],
            "--legacy no admite --void-eye",
        ),
        (
            vec![
                o("--legacy"),
                o("--void-eye"),
                o("--lucid-surface"),
                o("--output"),
                output.as_os_str(),
            ],
            "--legacy no admite",
        ),
        (
            [&creature[..], &compare[..]].concat(),
            "--void-eye solo se combina",
        ),
        (
            [&creature[..], &compare[..], &[o("--creature-distant-turn")]].concat(),
            "--creature-distant-turn",
        ),
        (
            [
                &creature[..],
                &compare[..],
                &[o("--creature-horizontal-follow")],
            ]
            .concat(),
            "--creature-horizontal-follow",
        ),
        (
            [&creature[..], &compare[..], &[o("--lucid-surface")]].concat(),
            "--creature-eye-compare",
        ),
        // Con el Sol con ojo el giro lejano sigue prohibido (lo rechaza su propio guard).
        (
            [
                &creature[..],
                &[o("--creature-solar-reveal"), o("--creature-distant-turn")],
            ]
            .concat(),
            "--creature-distant-turn",
        ),
    ];
    for (args, needle) in cases {
        let result = run_cli(&args);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "{args:?} debia fallar");
        assert!(stderr.contains(needle), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("no.obj") && !stderr.contains("no.png"),
            "el error debe llegar antes de leer los assets: {stderr}"
        );
        assert!(!output.exists(), "no debe escribir PNG: {args:?}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

use umbral::{
    app::{GameOptions, run_game_with_void_eye},
    visual::static_creature::CreatureOptions,
};

fn library_options(output: &std::path::Path) -> GameOptions {
    GameOptions {
        width: 320,
        height: 180,
        headless: true,
        frame_limit: None,
        output: output.to_path_buf(),
        time: 1.3,
        near: Some(3),
        culling: true,
        lod: false,
        overlay: false,
        creature: None,
        creature_reveal: false,
        creature_reveal_stages: false,
        creature_eye_follow: false,
        debug_oxygen: 0.05,
    }
}

/// Las cuatro combinaciones de la entrada nueva: (lucid, solar).
const ROUTES: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

#[test]
fn the_void_library_entry_has_the_command_line_guards_before_any_io_on_every_route() {
    let dir = temp_dir("library-guards");
    let output = dir.join("salida.png");
    // Assets desechables existentes (nunca los reales): un alias de --output no debe
    // sobrescribirlos.
    let (dummy_model, dummy) = (
        dir.join("criatura-desechable.obj"),
        dir.join("criatura-desechable.png"),
    );
    std::fs::write(&dummy_model, b"# modelo desechable\n").unwrap();
    std::fs::write(&dummy, b"textura desechable").unwrap();
    // Rutas inexistentes: si un guard fallara, el error seria de lectura y no el esperado.
    let missing = |near: bool| {
        Some(CreatureOptions {
            model: dir.join("no.obj"),
            texture: dir.join("no.png"),
            near,
        })
    };
    for (lucid, solar) in ROUTES {
        // La base de cada ruta es valida salvo por el campo del caso.
        let base = || GameOptions {
            near: if solar { None } else { Some(3) },
            creature: if solar { missing(false) } else { None },
            ..library_options(&output)
        };
        let alias = CreatureOptions {
            model: dummy_model.clone(),
            texture: dummy.clone(),
            near: false,
        };
        let mut cases: Vec<(GameOptions, &str)> = vec![
            (
                GameOptions {
                    time: f64::NAN,
                    ..base()
                },
                "--time necesita segundos >= 0",
            ),
            (
                GameOptions {
                    time: -1.0,
                    ..base()
                },
                "--time necesita segundos >= 0",
            ),
            (
                GameOptions {
                    time: f64::INFINITY,
                    ..base()
                },
                "--time necesita segundos >= 0",
            ),
            (
                GameOptions {
                    debug_oxygen: 1.5,
                    ..base()
                },
                "--debug-oxygen necesita un valor entre 0 y 1",
            ),
            (
                GameOptions {
                    debug_oxygen: f64::NAN,
                    ..base()
                },
                "--debug-oxygen necesita un valor entre 0 y 1",
            ),
            (
                GameOptions { width: 0, ..base() },
                "--width debe ser mayor que cero",
            ),
            (
                GameOptions {
                    height: 0,
                    ..base()
                },
                "--height debe ser mayor que cero",
            ),
            (
                GameOptions {
                    width: 5000,
                    height: 5000,
                    ..base()
                },
                "maximo",
            ),
            (
                GameOptions {
                    width: usize::MAX,
                    height: 2,
                    ..base()
                },
                "maximo",
            ),
            (
                GameOptions {
                    output: dir.join("salida.bmp"),
                    creature: missing(false),
                    near: None,
                    ..base()
                },
                "--output debe terminar en .png",
            ),
            (
                GameOptions {
                    output: dummy.clone(),
                    creature: Some(alias),
                    near: None,
                    ..base()
                },
                "coincide con la entrada",
            ),
        ];
        if solar {
            cases.extend([
                (
                    GameOptions {
                        creature: None,
                        ..base()
                    },
                    "--creature-solar-reveal necesita --creature-model y --creature-texture",
                ),
                (
                    GameOptions {
                        near: Some(3),
                        ..base()
                    },
                    "--near son incompatibles",
                ),
                (
                    GameOptions {
                        creature: missing(true),
                        ..base()
                    },
                    "--near-creature",
                ),
                (
                    GameOptions {
                        creature_reveal: true,
                        ..base()
                    },
                    "--creature-reveal",
                ),
            ]);
        } else {
            cases.extend([
                (
                    GameOptions {
                        creature: missing(true),
                        ..base()
                    },
                    "--near y --near-creature son incompatibles",
                ),
                (
                    GameOptions {
                        creature_reveal: true,
                        ..base()
                    },
                    "--creature-reveal necesita --creature-model y --creature-texture",
                ),
                (
                    GameOptions {
                        creature: missing(false),
                        near: None,
                        creature_reveal: true,
                        creature_reveal_stages: true,
                        ..base()
                    },
                    "son incompatibles: elige un modo",
                ),
                (
                    GameOptions {
                        creature: missing(false),
                        near: None,
                        creature_eye_follow: true,
                        ..base()
                    },
                    "--creature-eye-follow necesita --creature-reveal-stages",
                ),
            ]);
        }
        for (options, needle) in cases {
            let error = run_game_with_void_eye(options, lucid, solar).unwrap_err();
            assert!(
                error.contains(needle),
                "lucid {lucid} solar {solar}: {needle}: {error}"
            );
            assert!(
                !error.contains("no.obj") && !error.contains("no.png"),
                "{error}"
            );
            assert!(!output.exists(), "no debe escribir PNG");
            assert_eq!(std::fs::read(&dummy).unwrap(), b"textura desechable");
            assert_eq!(
                std::fs::read(&dummy_model).unwrap(),
                b"# modelo desechable\n"
            );
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

use umbral::visual::void_game::{VoidEyeRequest, validate_void_eye_flags};

#[test]
fn the_void_flag_validator_and_the_help_describe_its_routes() {
    let all = VoidEyeRequest {
        legacy: true,
        solar: true,
        eye_compare: true,
        distant_turn: true,
        horizontal_follow: true,
    };
    assert_eq!(validate_void_eye_flags(false, all), Ok(()));
    assert_eq!(
        validate_void_eye_flags(true, VoidEyeRequest::default()),
        Ok(())
    );
    // Con el Sol con ojo, los flags que la ruta solar ya implica siguen siendo validos.
    let solar = VoidEyeRequest {
        solar: true,
        eye_compare: true,
        horizontal_follow: true,
        ..VoidEyeRequest::default()
    };
    assert_eq!(validate_void_eye_flags(true, solar), Ok(()));
    assert!(
        validate_void_eye_flags(true, all)
            .unwrap_err()
            .contains("--legacy")
    );
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(help.status.success());
    assert!(text.contains("--void-eye"), "{text}");
    assert!(text.contains("Vigilia"), "{text}");
    // Las rutas previas siguen descritas.
    assert!(text.contains("--lucid-surface") && text.contains("--creature-solar-reveal"));
    // Sin rutas absolutas (unidad de Windows o carpeta de usuario): solo rutas genericas.
    for absolute in [":\\", ":/", "/Users", "\\Users"] {
        assert!(!text.contains(absolute), "la ayuda no lleva rutas privadas");
    }
}

// --- Guardas de comportamiento y preservacion (nacidas en verde; ver mutaciones en la evidencia).

use umbral::{
    app::advance_frame,
    math::{Vec2, Vec3},
    renderer::{
        shader::{FragmentInput, Material, ShaderContext, shade_fragment},
        stats::RenderStats,
    },
    scene::ship::ShipInput,
    visual::lucid_game::set_lucid_surface,
};

#[test]
fn only_vigilia_changes_shader_keeping_its_real_material_and_disabling_restores_the_world() {
    let original = new_world(320, 180);
    let mut world = new_world(320, 180);
    assert_eq!(set_void_eye(&mut world, true), Ok(3));
    assert_eq!(set_void_eye(&mut world, true), Ok(3), "idempotente");
    let mut expected = new_world(320, 180);
    expected.system.bodies[3].shader = ShaderKind::VoidEye;
    assert_eq!(snapshot(&world), snapshot(&expected));
    // Material real de Vigilia (no el gris del preview), radio, orbita, rotacion y fase.
    let vigilia = &world.system.bodies[3];
    let material = Material::solid(0.30, 0.38, 0.55);
    assert_eq!(vigilia.material.base_color, material.base_color);
    assert_eq!(vigilia.material.ambient, material.ambient);
    assert_eq!(vigilia.material.cull_backfaces, material.cull_backfaces);
    assert_eq!(vigilia.body_radius, 20.0);
    assert_eq!(vigilia.orbit.radius, 520.0);
    assert_eq!(vigilia.orbit.angular_speed, 0.025);
    assert_eq!(vigilia.orbit.axial_speed, 0.25);
    assert_eq!(vigilia.phase, 4.4);
    for enabled in [false, false] {
        assert_eq!(set_void_eye(&mut world, enabled), Ok(3));
        assert_eq!(snapshot(&world), snapshot(&original));
    }
    // Con la superficie lucida puesta, cada flag toca solo su planeta.
    let mut both = new_world(320, 180);
    set_lucid_surface(&mut both, true).unwrap();
    set_void_eye(&mut both, true).unwrap();
    let mut reference = new_world(320, 180);
    reference.system.bodies[1].shader = ShaderKind::LucidSurface;
    reference.system.bodies[3].shader = ShaderKind::VoidEye;
    assert_eq!(snapshot(&both), snapshot(&reference));
}

/// El juego normal junto a Vigilia (`--near 3`), como lo arma la CLI.
fn game_world(oxygen: f64, time: f64, lucid: bool, void: bool, near: usize) -> World {
    let mut world = new_world(320, 180);
    world.time = time;
    world.debug_oxygen01 = oxygen;
    world.toggles.lod = false;
    if lucid {
        set_lucid_surface(&mut world, true).unwrap();
    }
    if void {
        set_void_eye(&mut world, true).unwrap();
    }
    world.place_near(near, 25.0).unwrap();
    world
}

fn frame(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Pixeles `0xRRGGBB` de un PNG RGB de 8 bits.
fn decode(path: &std::path::Path) -> (usize, usize, Vec<u32>) {
    let decoder = png::Decoder::new(BufReader::new(File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    let pixels = bytes[..info.buffer_size()]
        .chunks_exact(3)
        .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
        .collect();
    (info.width as usize, info.height as usize, pixels)
}

fn cli_frame(dir: &std::path::Path, name: &str, extra: &[&str]) -> Vec<u32> {
    let output = dir.join(name);
    let o = std::ffi::OsStr::new;
    let mut args = vec![
        o("--width"),
        o("320"),
        o("--height"),
        o("180"),
        o("--no-lod"),
        o("--output"),
        output.as_os_str(),
    ];
    args.extend(extra.iter().map(|a| o(a)));
    let result = run_cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let (w, h, pixels) = decode(&output);
    assert_eq!((w, h), (320, 180));
    pixels
}

fn background(pixels: &[u32]) -> Vec<bool> {
    pixels.iter().map(|&p| p == SPACE_COLOR).collect()
}

/// Vigilia se ve en el cuadro si pintarla de otro color (shader `Flat` rojo) lo cambia; se
/// restaura despues. No depende del ojo del vacio.
fn vigilia_visible(world: &mut World, draw: &dyn Fn(&World) -> Vec<u32>) -> bool {
    let index = umbral::visual::void_game::void_body_index(&world.system).unwrap();
    let before = draw(world);
    let (material, shader) = (
        world.system.bodies[index].material,
        world.system.bodies[index].shader,
    );
    world.system.bodies[index].material = Material::solid(1.0, 0.0, 0.0);
    world.system.bodies[index].shader = ShaderKind::Flat;
    let painted = draw(world);
    world.system.bodies[index].material = material;
    world.system.bodies[index].shader = shader;
    painted != before
}

#[test]
fn the_cli_frames_show_vigilia_with_the_eye_on_both_normal_routes_and_nothing_else_changes() {
    let dir = temp_dir("cli-frames");
    let mut by_oxygen = Vec::new();
    for (time, t) in [("1.3", 1.3), ("2.8", 2.8)] {
        for (oxygen, o2) in [("1", 1.0), ("0.05", 0.05)] {
            let base = ["--near", "3", "--time", time, "--debug-oxygen", oxygen];
            let with = cli_frame(
                &dir,
                &format!("con-{time}-{oxygen}.png"),
                &[&base[..], &["--void-eye"]].concat(),
            );
            let without = cli_frame(&dir, &format!("sin-{time}-{oxygen}.png"), &base);
            let (expected, void_stats) = frame(&game_world(o2, t, false, true, 3));
            let (original, original_stats) = frame(&game_world(o2, t, false, false, 3));
            assert!(
                with == expected,
                "t {time} O2 {oxygen}: Vigilia con VoidEye02"
            );
            assert!(
                without == original,
                "t {time} O2 {oxygen}: sin flag, el juego de siempre"
            );
            let changed = with.iter().zip(&without).filter(|(a, b)| a != b).count();
            println!(
                "near 3, t {time}, O2 {oxygen}: {changed} pixeles distintos del juego sin flag"
            );
            assert!(changed > 0, "Vigilia se ve de cerca: el ojo debe verse");
            assert_eq!(
                void_stats, original_stats,
                "misma geometria, LOD y descarte"
            );
            assert!(background(&with) == background(&without), "misma silueta");
            by_oxygen.push((time, oxygen, with));
        }
    }
    // El ojo no depende del O2: a igual tiempo, el cuadro con 100 % y con 5 % es el mismo.
    assert!(by_oxygen[0].2 == by_oxygen[1].2 && by_oxygen[2].2 == by_oxygen[3].2);
    // La corona respira con el reloj de simulacion: 1.3 s y 2.8 s difieren.
    assert!(by_oxygen[0].2 != by_oxygen[2].2);
    // Normal + Lucid + Void: cada planeta con su shader, junto a Vigilia y junto a Antena.
    for near in ["3", "1"] {
        let index: usize = near.parse().unwrap();
        let args = [
            "--near",
            near,
            "--time",
            "1.3",
            "--debug-oxygen",
            "0.05",
            "--lucid-surface",
            "--void-eye",
        ];
        let both = cli_frame(&dir, &format!("ambos-{near}.png"), &args);
        let (expected, _) = frame(&game_world(0.05, 1.3, true, true, index));
        let (lucid_only, _) = frame(&game_world(0.05, 1.3, true, false, index));
        assert!(
            both == expected,
            "near {near}: Antena lucida y Vigilia con el ojo"
        );
        let mut probe = game_world(0.05, 1.3, true, true, index);
        let visible = vigilia_visible(&mut probe, &|w| frame(w).0);
        println!(
            "near {near}: Vigilia visible {visible}; distinto de solo lucida {}",
            both != lucid_only
        );
        assert_eq!(
            both != lucid_only,
            visible,
            "near {near}: el ojo cambia el cuadro solo si Vigilia se ve"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Respuesta del shader de Vigilia en puntos fijos de su superficie, con el contexto que arma
/// `World` a partir de su estado publico (tiempo, O2, camara, nave y luz).
fn vigilia_response(world: &World) -> Vec<(u64, u64, u64)> {
    let index = umbral::visual::void_game::void_body_index(&world.system).unwrap();
    let body = &world.system.bodies[index];
    let center = body.position(world.time);
    let context = ShaderContext {
        elapsed_time: world.time,
        oxygen01: world.debug_oxygen01.clamp(0.0, 1.0),
        camera_position: world.camera.position,
        ship_position: world.ship.position,
        light_position: world.system.light_position(),
    };
    let mut out = Vec::new();
    for i in 0..24 {
        for j in 1..12 {
            let (theta, phi) = (
                j as f64 / 12.0 * std::f64::consts::PI,
                i as f64 / 24.0 * std::f64::consts::TAU,
            );
            let normal = Vec3::new(
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            );
            let f = FragmentInput {
                screen_position: Vec3::ZERO,
                world_position: center + normal * body.body_radius,
                normal,
                uv: Vec2::new(i as f64 / 24.0, j as f64 / 12.0),
            };
            let c = shade_fragment(body.shader, &f, &context, &body.material);
            out.push((c.x.to_bits(), c.y.to_bits(), c.z.to_bits()));
        }
    }
    out
}

#[test]
fn in_the_real_loop_the_eye_follows_the_moving_ship_and_not_the_camera() {
    // Mismas entradas al mundo con el ojo y a la referencia (el de siempre con solo
    // `bodies[3].shader` cambiado): cada cuadro coincide.
    let mut void = game_world(1.0, 1.3, false, true, 3);
    let mut reference = game_world(1.0, 1.3, false, false, 3);
    reference.system.bodies[3].shader = ShaderKind::VoidEye;
    let thrust = ShipInput {
        thrust: 0.6,
        yaw: 0.2,
        ..ShipInput::default()
    };
    let mut previous = vigilia_response(&void);
    for step in 0..6 {
        let ship = void.ship.position;
        for world in [&mut void, &mut reference] {
            advance_frame(world, &thrust, 1.0 / 30.0);
        }
        assert!(frame(&void).0 == frame(&reference).0, "cuadro {step}");
        assert!(void.ship.position != ship, "la nave se mueve");
        let response = vigilia_response(&void);
        assert!(
            response != previous,
            "cuadro {step}: la respuesta sigue a la nave/reloj"
        );
        previous = response;
    }
}

#[test]
fn f5_freezes_the_breathing_clock_while_the_eye_keeps_following_the_ship() {
    // Reloj de simulacion congelado (F5) y nave quieta: ni la respuesta ni el cuadro cambian.
    let mut frozen = game_world(1.0, 1.3, false, true, 3);
    frozen.toggles.freeze_orbits = true;
    let (time, ship) = (frozen.time, frozen.ship.position);
    let (response, still) = (vigilia_response(&frozen), frame(&frozen).0);
    for _ in 0..5 {
        advance_frame(&mut frozen, &ShipInput::default(), 1.0 / 30.0);
    }
    assert_eq!((frozen.time, frozen.ship.position), (time, ship));
    assert!(
        vigilia_response(&frozen) == response,
        "F5 congela la respiracion"
    );
    assert!(frame(&frozen).0 == still);
    // Sin F5 el mismo paso avanza el reloj y la corona respira (la nave sigue quieta).
    let mut running = game_world(1.0, 1.3, false, true, 3);
    advance_frame(&mut running, &ShipInput::default(), 1.0 / 30.0);
    assert!(running.time > 1.3 && running.ship.position == ship);
    assert!(
        vigilia_response(&running) != response,
        "sin F5 la corona respira"
    );
    // Con F5, si la nave se mueve el ojo la sigue (el reloj no avanza).
    let thrust = ShipInput {
        thrust: 0.6,
        ..ShipInput::default()
    };
    advance_frame(&mut frozen, &thrust, 1.0 / 30.0);
    advance_frame(&mut frozen, &thrust, 1.0 / 30.0);
    assert_eq!(frozen.time, time);
    assert!(frozen.ship.position != ship);
    assert!(
        vigilia_response(&frozen) != response,
        "con F5 el ojo sigue a la nave"
    );
}

#[test]
fn reorienting_the_camera_without_moving_the_ship_does_not_change_the_eye() {
    let mut parked = game_world(1.0, 1.3, false, true, 3);
    let before = vigilia_response(&parked);
    let (ship, camera) = (parked.ship.position, parked.camera.position);
    parked.camera.target += Vec3::new(40.0, -15.0, 25.0);
    parked.camera.position += Vec3::new(-30.0, 12.0, 8.0);
    assert_eq!(parked.ship.position, ship);
    assert!(parked.camera.position != camera);
    assert!(
        vigilia_response(&parked) == before,
        "la camara no es el objetivo"
    );
    // Y con la camara movida a la nave, el ojo mira a la nave, no a la camara (control).
    let mut moved = game_world(1.0, 1.3, false, true, 3);
    moved.ship.position += Vec3::new(30.0, 0.0, 0.0);
    assert!(vigilia_response(&moved) != before);
}

// Criatura sintetica desechable (la cupula calibrada de `tests/solar_game.rs`), nunca el asset
// real: sirve para las rutas con el Sol con ojo.
use umbral::{
    assets::obj::parse_obj,
    renderer::texture::Texture,
    visual::{
        solar_game::{SolarGame, SolarKeys, solar_creature_placement},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

fn dome_source() -> String {
    let segments = 16;
    let mut s = String::from("vt 0.5 0.5\nv 0 0 1\nvn 0 0 1\n");
    let mut index = vec![vec![1usize]];
    let mut next = 2;
    for k in 1..=8 {
        let phi = (k as f64 * 10.0).to_radians();
        let mut ring = Vec::new();
        for j in 0..segments {
            let t = (j as f64 / segments as f64) * std::f64::consts::TAU;
            let (x, y, z) = (phi.sin() * t.cos(), phi.sin() * t.sin(), phi.cos());
            s += &format!("v {x} {y} {z}\nvn {x} {y} {z}\n");
            ring.push(next);
            next += 1;
        }
        index.push(ring);
    }
    let f = |a: usize, b: usize, c: usize| format!("f {a}/1/{a} {b}/1/{b} {c}/1/{c}\n");
    for j in 0..segments {
        s += &f(1, index[1][j], index[1][(j + 1) % segments]);
    }
    for k in 1..8 {
        for j in 0..segments {
            let (a, b) = (index[k][j], index[k][(j + 1) % segments]);
            let (c, d) = (index[k + 1][j], index[k + 1][(j + 1) % segments]);
            s += &f(a, c, d);
            s += &f(a, d, b);
        }
    }
    s
}

fn calibrated_source() -> String {
    let source = dome_source()
        + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n";
    let (c, s) = (
        CREATURE_EYE_REGION.center(),
        CREATURE_EYE_REGION.radius() / 0.85,
    );
    let mut out = String::new();
    for line in source.lines() {
        match line.strip_prefix("v ") {
            Some(rest) => {
                let p: Vec<f64> = rest.split(' ').map(|x| x.parse().unwrap()).collect();
                let q = c + (Vec3::new(p[0], p[1], p[2]) - Vec3::Z) * s;
                out += &format!("v {} {} {}\n", q.x, q.y, q.z);
            }
            None => out += &format!("{line}\n"),
        }
    }
    out
}

const TEXELS: [u8; 12] = [200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160];

fn fixture_creature(placement: CreaturePlacement) -> StaticCreature {
    let texture = Texture::from_rgb8(2, 2, TEXELS.to_vec()).unwrap();
    StaticCreature::new(parse_obj(&calibrated_source()).unwrap(), texture, placement).unwrap()
}

/// El arranque del Sol con ojo como lo hace el juego: O2 y tiempo puestos, superficie lucida y
/// ojo del vacio (si se piden) y `SolarGame::start`.
fn solar_world(oxygen: f64, time: f64, lucid: bool, void: bool) -> (World, SolarGame) {
    let mut world = new_world(320, 180);
    world.time = time;
    world.debug_oxygen01 = oxygen;
    if lucid {
        set_lucid_surface(&mut world, true).unwrap();
    }
    if void {
        set_void_eye(&mut world, true).unwrap();
    }
    let placement = solar_creature_placement(&world).unwrap();
    let game = SolarGame::start(&mut world, fixture_creature(placement)).unwrap();
    (world, game)
}

fn solar_pixels(world: &World, game: &SolarGame) -> Vec<u32> {
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    game.render(world, &mut fb).unwrap();
    fb.pixels
}

/// Modelo y textura desechables (la misma cupula) en `dir`.
fn write_fixture(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let (model, texture) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    let file = File::create(&texture).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&TEXELS).unwrap();
    writer.finish().unwrap();
    (model, texture)
}

const SOLAR_TIME: &str = "10";

#[test]
fn the_solar_routes_draw_vigilia_with_the_eye_only_where_it_is_visible() {
    let dir = temp_dir("cli-solar");
    let (model, texture) = write_fixture(&dir);
    let o = std::ffi::OsStr::new;
    // En FRONTAL a t = 10 Vigilia queda fuera del cuadro (la igualdad es correcta); se busca
    // ademas el primer tiempo en que si se ve, para comprobar el ojo en la ruta solar real.
    let visible_time = (0..64)
        .map(|k| f64::from(k) * 4.0)
        .find(|&t| {
            let (mut world, game) = solar_world(0.05, t, false, true);
            vigilia_visible(&mut world, &|w| solar_pixels(w, &game))
        })
        .expect("algun tiempo con Vigilia en el encuadre FRONTAL");
    println!("primer tiempo con Vigilia visible en FRONTAL: {visible_time} s");
    for (time, lucid) in [
        (10.0, false),
        (10.0, true),
        (visible_time, false),
        (visible_time, true),
    ] {
        let clock = format!("{time}");
        let output = dir.join(format!("solar-{time}-{lucid}.png"));
        let mut args = vec![
            o("--creature-model"),
            model.as_os_str(),
            o("--creature-texture"),
            texture.as_os_str(),
            o("--creature-solar-reveal"),
            o("--void-eye"),
            o("--debug-oxygen"),
            o("0.05"),
            o("--time"),
            o(&clock),
            o("--width"),
            o("320"),
            o("--height"),
            o("180"),
            o("--output"),
            output.as_os_str(),
        ];
        if lucid {
            args.push(o("--lucid-surface"));
        }
        let result = run_cli(&args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let (world, game) = solar_world(0.05, time, lucid, true);
        let (plain_world, plain_game) = solar_world(0.05, time, lucid, false);
        let expected = solar_pixels(&world, &game);
        let plain = solar_pixels(&plain_world, &plain_game);
        let (w, h, pixels) = decode(&output);
        assert_eq!((w, h), (320, 180));
        assert!(
            pixels == expected,
            "lucid {lucid}: el cuadro solar esperado"
        );
        let mut probe = solar_world(0.05, time, lucid, true);
        let draw = |w: &World| solar_pixels(w, &probe.1);
        let visible = vigilia_visible(&mut probe.0, &draw);
        println!(
            "Sol con ojo FRONTAL, t {time}, lucid {lucid}: Vigilia visible {visible}; distinto sin el ojo {}",
            expected != plain
        );
        assert_eq!(
            expected != plain,
            visible,
            "el ojo cambia el cuadro solo si Vigilia se ve"
        );
        assert_eq!(visible, time == visible_time, "t {time}");
    }
    // Vista OBLICUA (tecla 8) de la misma composicion: se registra si Vigilia se ve y, si se ve,
    // el ojo cambia el cuadro.
    let (mut world, mut game) = solar_world(0.05, 10.0, false, true);
    let (mut plain_world, mut plain_game) = solar_world(0.05, 10.0, false, false);
    let oblique = SolarKeys {
        oblique: true,
        ..SolarKeys::default()
    };
    game.apply(&mut world, oblique).unwrap();
    plain_game.apply(&mut plain_world, oblique).unwrap();
    let (a, b) = (
        solar_pixels(&world, &game),
        solar_pixels(&plain_world, &plain_game),
    );
    let draw = |w: &World| solar_pixels(w, &game);
    let visible = vigilia_visible(&mut world, &draw);
    println!(
        "Sol con ojo OBLICUA: Vigilia visible {visible}; distinto sin el ojo {}",
        a != b
    );
    assert_eq!(a != b, visible);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_solar_g_key_changes_the_creature_target_but_never_the_void_eye_target() {
    let (mut world, mut game) = solar_world(1.0, 10.0, false, true);
    let before = vigilia_response(&world);
    let target = world.creature_eye_target();
    let g = SolarKeys {
        target: true,
        ..SolarKeys::default()
    };
    game.apply(&mut world, g).unwrap();
    assert!(
        world.creature_eye_target() != target,
        "G cambia la mirada de la criatura"
    );
    assert!(
        vigilia_response(&world) == before,
        "G no cambia el ojo del vacio"
    );
    game.apply(&mut world, g).unwrap();
    assert_eq!(world.creature_eye_target(), target);
    assert!(vigilia_response(&world) == before);
}

#[test]
fn the_solar_library_entry_and_redundant_flags_give_the_expected_frame() {
    let dir = temp_dir("solar-entry");
    let (model, texture) = write_fixture(&dir);
    for lucid in [false, true] {
        let output = dir.join(format!("biblioteca-{lucid}.png"));
        let options = GameOptions {
            time: 10.0,
            near: None,
            lod: true,
            debug_oxygen: 0.25,
            creature: Some(CreatureOptions {
                model: model.clone(),
                texture: texture.clone(),
                near: false,
            }),
            ..library_options(&output)
        };
        run_game_with_void_eye(options, lucid, true).unwrap();
        let (world, game) = solar_world(0.25, 10.0, lucid, true);
        let expected = solar_pixels(&world, &game);
        assert!(
            decode(&output).2 == expected,
            "entrada de biblioteca, lucid {lucid}"
        );
        // CLI con los flags que la ruta solar ya implica: el mismo cuadro.
        let redundant = dir.join(format!("redundantes-{lucid}.png"));
        let o = std::ffi::OsStr::new;
        let mut args = vec![
            o("--creature-model"),
            model.as_os_str(),
            o("--creature-texture"),
            texture.as_os_str(),
            o("--creature-solar-reveal"),
            o("--void-eye"),
            o("--creature-reveal-stages"),
            o("--creature-eye-follow"),
            o("--creature-eye-compare"),
            o("--creature-horizontal-follow"),
            o("--debug-oxygen"),
            o("0.25"),
            o("--time"),
            o(SOLAR_TIME),
            o("--width"),
            o("320"),
            o("--height"),
            o("180"),
            o("--output"),
            redundant.as_os_str(),
        ];
        if lucid {
            args.push(o("--lucid-surface"));
        }
        let result = run_cli(&args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            decode(&redundant).2 == expected,
            "flags redundantes, lucid {lucid}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_plain_route_also_accepts_the_static_creature_and_its_reveal() {
    let dir = temp_dir("plain-creature");
    let (model, texture) = write_fixture(&dir);
    for (extra, name) in [
        (&["--creature-reveal"][..], "v1"),
        (
            &["--creature-reveal-stages", "--creature-eye-follow"][..],
            "etapas",
        ),
    ] {
        let output = dir.join(format!("{name}.png"));
        let o = std::ffi::OsStr::new;
        let mut args = vec![
            o("--creature-model"),
            model.as_os_str(),
            o("--creature-texture"),
            texture.as_os_str(),
            o("--void-eye"),
            o("--lucid-surface"),
            o("--debug-oxygen"),
            o("0.05"),
            o("--width"),
            o("320"),
            o("--height"),
            o("180"),
            o("--output"),
            output.as_os_str(),
        ];
        args.extend(extra.iter().map(|a| o(a)));
        let result = run_cli(&args);
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(decode(&output).0, 320);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
