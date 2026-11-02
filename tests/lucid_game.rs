//! Supervisor-prepared integration tracer bullet. The expected frame uses the approved shader.
use std::{fs::File, io::BufReader, process::Command};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::{framebuffer::Framebuffer, shader::ShaderKind},
    scene::world::{SPACE_COLOR, World},
};

#[test]
fn lucid_flag_renders_antena_with_the_approved_shader_in_the_real_game() {
    let (width, height) = (320, 180);
    let path = std::env::temp_dir().join(format!(
        "umbral_lucid_game_first_{}.png",
        std::process::id()
    ));
    let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--lucid-surface",
            "--headless",
            "--width",
            "320",
            "--height",
            "180",
            "--near",
            "1",
            "--debug-oxygen",
            "0.05",
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
        "the real game must accept --lucid-surface: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let ship = load_model(&ship_model_path()).unwrap();
    let mut expected_world = World::new(ship, width as f64 / height as f64).unwrap();
    expected_world.time = 1.3;
    expected_world.debug_oxygen01 = 0.05;
    expected_world.toggles.lod = false;
    expected_world.place_near(1, 25.0).unwrap();
    assert_eq!(expected_world.system.bodies[1].name, "Antena");
    expected_world.system.bodies[1].shader = ShaderKind::LucidSurface;
    let mut expected = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    expected_world.render(&mut expected);
    let expected_rgb: Vec<u8> = expected
        .pixels
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect();
    let decoder = png::Decoder::new(BufReader::new(File::open(&path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (width as u32, height as u32));
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert!(
        bytes[..info.buffer_size()] == expected_rgb,
        "the CLI must use the unchanged accepted shader on Antena only, in World::render"
    );
    std::fs::remove_file(path).unwrap();
}

// --- Bronya: incrementos RED/GREEN y guardas de la integracion (ver la evidencia). ---

use umbral::{
    renderer::shader::Material, scene::solar_system::SolarSystem,
    visual::lucid_game::set_lucid_surface,
};

fn new_world(width: usize, height: usize) -> World {
    World::new(
        load_model(&ship_model_path()).unwrap(),
        width as f64 / height as f64,
    )
    .unwrap()
}

/// Todo lo observable del mundo que la superficie lucida no debe tocar (y el sistema entero).
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
fn the_target_is_validated_before_any_change_and_errors_leave_the_world_intact() {
    type Edit = fn(&mut SolarSystem);
    let cases: [(&str, Edit, &str); 8] = [
        ("sin Antena", |s| s.bodies[1].name = "Otra", "Antena"),
        (
            "Antena duplicada",
            |s| s.bodies[2].name = "Antena",
            "ambigu",
        ),
        (
            "Antena es el Sol",
            |s| s.bodies[1].shader = ShaderKind::AnimatedSun,
            "shader",
        ),
        (
            "radio no finito",
            |s| s.bodies[1].body_radius = f64::NAN,
            "radio",
        ),
        ("radio 0", |s| s.bodies[1].body_radius = 0.0, "radio"),
        ("sin orbita", |s| s.bodies[1].orbit.radius = 0.0, "orbita"),
        (
            "orbita infinita",
            |s| s.bodies[1].orbit.radius = f64::INFINITY,
            "orbita",
        ),
        (
            "shader ajeno",
            |s| s.bodies[1].shader = ShaderKind::VoidEye,
            "shader",
        ),
    ];
    for (name, edit, needle) in cases {
        for enabled in [true, false] {
            let mut world = new_world(64, 36);
            edit(&mut world.system);
            let before = snapshot(&world);
            let error = set_lucid_surface(&mut world, enabled).unwrap_err();
            assert!(error.contains(needle), "{name}: {error}");
            assert_eq!(
                snapshot(&world),
                before,
                "{name}: el Err no debe mutar nada"
            );
        }
    }
    // Reordenado: se elige por identidad (nombre), no por un indice fijo.
    let mut world = new_world(64, 36);
    world.system.bodies.swap(1, 3);
    assert_eq!(set_lucid_surface(&mut world, true), Ok(3));
    assert_eq!(world.system.bodies[3].name, "Antena");
    assert_eq!(world.system.bodies[3].shader, ShaderKind::LucidSurface);
    assert_eq!(world.system.bodies[1].shader, ShaderKind::LitPlanet);
}

#[test]
fn only_antena_changes_shader_and_disabling_restores_the_original_world() {
    let original = new_world(320, 180);
    let mut world = new_world(320, 180);
    assert_eq!(set_lucid_surface(&mut world, true), Ok(1));
    assert_eq!(set_lucid_surface(&mut world, true), Ok(1), "idempotente");
    let mut expected = new_world(320, 180);
    expected.system.bodies[1].shader = ShaderKind::LucidSurface;
    assert_eq!(snapshot(&world), snapshot(&expected));
    // Material, radio, orbita, fase y rotacion de Antena: los originales.
    let antena = &world.system.bodies[1];
    let material = Material::solid(0.45, 0.40, 0.36);
    assert_eq!(antena.material.base_color, material.base_color);
    assert_eq!(antena.material.ambient, material.ambient);
    assert_eq!(antena.body_radius, 14.0);
    assert_eq!(antena.orbit.radius, 220.0);
    for enabled in [false, false] {
        assert_eq!(set_lucid_surface(&mut world, enabled), Ok(1));
        assert_eq!(snapshot(&world), snapshot(&original));
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-lucid-game-{name}-{}", std::process::id()));
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
fn the_command_line_rejects_lucid_combinations_outside_its_two_routes_before_any_io() {
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
        o("--lucid-surface"),
    ];
    let compare = [
        o("--creature-reveal-stages"),
        o("--creature-eye-follow"),
        o("--creature-eye-compare"),
    ];
    let mut cases: Vec<(Vec<&std::ffi::OsStr>, &str)> = vec![
        (
            vec![
                o("--legacy"),
                o("--lucid-surface"),
                o("--output"),
                output.as_os_str(),
            ],
            "--legacy no admite --lucid-surface",
        ),
        (
            [&creature[..], &compare[..]].concat(),
            "--creature-eye-compare",
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
    ];
    // Con el Sol con ojo el giro lejano sigue prohibido (lo rechaza su propio guard).
    cases.push((
        [
            &creature[..],
            &[o("--creature-solar-reveal"), o("--creature-distant-turn")],
        ]
        .concat(),
        "--creature-distant-turn",
    ));
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

// Criatura sintetica desechable (la cupula calibrada de `tests/solar_game.rs`), nunca el asset
// real: sirve para la ruta combinada con el Sol con ojo.
use umbral::{
    assets::obj::parse_obj,
    math::Vec3,
    renderer::{stats::RenderStats, texture::Texture},
    visual::{
        solar_game::{SolarGame, solar_creature_placement},
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

/// El arranque del Sol con ojo como lo hace el juego: O2 y tiempo puestos, superficie lucida
/// (si `lucid`) y `SolarGame::start`.
fn solar_world(oxygen: f64, time: f64, lucid: bool) -> (World, SolarGame) {
    let mut world = new_world(320, 180);
    world.time = time;
    world.debug_oxygen01 = oxygen;
    if lucid {
        set_lucid_surface(&mut world, true).unwrap();
    }
    let placement = solar_creature_placement(&world).unwrap();
    let game = SolarGame::start(&mut world, fixture_creature(placement)).unwrap();
    (world, game)
}

fn solar_frame(world: &World, game: &SolarGame) -> (Framebuffer, RenderStats) {
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    let stats = game.render(world, &mut fb).unwrap();
    (fb, stats)
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

/// Antena se ve en el encuadre FRONTAL del Sol con ojo a t = 10 s (comprobado en la evidencia).
const SOLAR_TIME: &str = "10";

#[test]
fn the_solar_route_with_the_flag_draws_antena_lucid_in_the_approved_composition() {
    let dir = temp_dir("cli-solar");
    let (model, texture) = write_fixture(&dir);
    let output = dir.join("solar.png");
    let o = std::ffi::OsStr::new;
    let result = run_cli(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
        o("--creature-solar-reveal"),
        o("--lucid-surface"),
        o("--debug-oxygen"),
        o("0.05"),
        o("--time"),
        o(SOLAR_TIME),
        o("--width"),
        o("320"),
        o("--height"),
        o("180"),
        o("--output"),
        output.as_os_str(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let (lucid_world, lucid_game) = solar_world(0.05, 10.0, true);
    let (plain_world, plain_game) = solar_world(0.05, 10.0, false);
    let (expected, _) = solar_frame(&lucid_world, &lucid_game);
    let (plain, _) = solar_frame(&plain_world, &plain_game);
    assert!(
        expected.pixels != plain.pixels,
        "Antena debe verse en el encuadre"
    );
    let (w, h, pixels) = decode(&output);
    assert_eq!((w, h), (320, 180));
    assert!(
        pixels == expected.pixels,
        "la ruta solar debe dibujar Antena con la superficie lucida y nada mas distinto"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

use umbral::{
    app::{GameOptions, run_game_with_lucid_surface, run_game_with_solar_and_lucid_surface},
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
        near: Some(1),
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

#[test]
fn the_new_library_entries_have_the_command_line_guards_before_any_io() {
    let dir = temp_dir("library-guards");
    let output = dir.join("salida.png");
    // Rutas inexistentes: si un guard fallara, el error seria de lectura y no el esperado.
    let missing = |near: bool| {
        Some(CreatureOptions {
            model: dir.join("no.obj"),
            texture: dir.join("no.png"),
            near,
        })
    };
    let base = || library_options(&output);
    let common: Vec<(GameOptions, &str)> = vec![
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
    ];
    let plain: Vec<(GameOptions, &str)> = vec![
        (
            GameOptions {
                creature: missing(false),
                output: dir.join("salida.bmp"),
                ..base()
            },
            "--output debe terminar en .png",
        ),
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
    ];
    let solar = |options: GameOptions| GameOptions {
        creature: missing(false),
        near: None,
        ..options
    };
    for (options, needle) in common.into_iter().chain(plain) {
        let error = run_game_with_lucid_surface(options).unwrap_err();
        assert!(error.contains(needle), "{needle}: {error}");
        assert!(!output.exists());
    }
    let solar_cases: Vec<(GameOptions, &str)> = vec![
        (
            GameOptions {
                time: f64::INFINITY,
                ..solar(base())
            },
            "--time necesita segundos >= 0",
        ),
        (
            GameOptions {
                debug_oxygen: -0.1,
                ..solar(base())
            },
            "--debug-oxygen necesita un valor entre 0 y 1",
        ),
        (
            GameOptions {
                width: 0,
                ..solar(base())
            },
            "--width debe ser mayor que cero",
        ),
        (
            library_options(&output),
            "--creature-solar-reveal necesita --creature-model y --creature-texture",
        ),
        (
            base_near(&output, missing(false)),
            "--near son incompatibles",
        ),
        (
            GameOptions {
                output: dir.join("salida.bmp"),
                ..solar(base())
            },
            "--output debe terminar en .png",
        ),
    ];
    for (options, needle) in solar_cases {
        let error = run_game_with_solar_and_lucid_surface(options).unwrap_err();
        assert!(error.contains(needle), "{needle}: {error}");
        assert!(!output.exists());
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

fn base_near(output: &std::path::Path, creature: Option<CreatureOptions>) -> GameOptions {
    GameOptions {
        creature,
        ..library_options(output)
    }
}

use umbral::visual::lucid_game::{LucidSurfaceRequest, validate_lucid_surface_flags};

#[test]
fn the_flag_validator_and_the_help_describe_the_two_routes() {
    let all = LucidSurfaceRequest {
        legacy: true,
        solar: true,
        eye_compare: true,
        distant_turn: true,
        horizontal_follow: true,
    };
    assert_eq!(validate_lucid_surface_flags(false, all), Ok(()));
    assert_eq!(
        validate_lucid_surface_flags(true, LucidSurfaceRequest::default()),
        Ok(())
    );
    // Con el Sol con ojo, los flags que la ruta solar ya implica siguen siendo validos.
    let solar = LucidSurfaceRequest {
        solar: true,
        eye_compare: true,
        horizontal_follow: true,
        ..LucidSurfaceRequest::default()
    };
    assert_eq!(validate_lucid_surface_flags(true, solar), Ok(()));
    assert!(
        validate_lucid_surface_flags(true, all)
            .unwrap_err()
            .contains("--legacy")
    );
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(help.status.success());
    assert!(text.contains("--lucid-surface"), "{text}");
    assert!(text.contains("Antena"), "{text}");
    // Sin rutas absolutas (unidad de Windows o carpeta de usuario): solo rutas genericas.
    for absolute in [":\\", ":/", "/Users", "\\Users"] {
        assert!(!text.contains(absolute), "la ayuda no lleva rutas privadas");
    }
}

// --- Guardas de comportamiento y preservacion (nacidas en verde; ver mutaciones en la evidencia).

use umbral::{
    app::advance_frame, scene::ship::ShipInput, visual::creature_reveal::step_debug_oxygen,
};

/// Cuadro del juego normal arrancando junto a Antena (`--near 1`), como lo arma la CLI.
fn game_world(oxygen: f64, time: f64, lucid: bool) -> World {
    let mut world = new_world(320, 180);
    world.time = time;
    world.debug_oxygen01 = oxygen;
    world.toggles.lod = false;
    if lucid {
        set_lucid_surface(&mut world, true).unwrap();
    }
    world.place_near(1, 25.0).unwrap();
    world
}

fn frame(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

fn cli_frame(dir: &std::path::Path, name: &str, oxygen: &str, lucid: bool) -> Vec<u32> {
    let output = dir.join(name);
    let o = std::ffi::OsStr::new;
    let mut args = vec![
        o("--width"),
        o("320"),
        o("--height"),
        o("180"),
        o("--near"),
        o("1"),
        o("--debug-oxygen"),
        o(oxygen),
        o("--time"),
        o("1.3"),
        o("--no-lod"),
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
    decode(&output).2
}

fn background(pixels: &[u32]) -> Vec<bool> {
    pixels.iter().map(|&p| p == SPACE_COLOR).collect()
}

#[test]
fn the_cli_frames_are_the_original_game_at_high_oxygen_and_antena_lucid_below_half() {
    let dir = temp_dir("cli-levels");
    for (oxygen, value) in [("1", 1.0), ("0.6", 0.6), ("0.5", 0.5)] {
        let with = cli_frame(&dir, &format!("con-{oxygen}.png"), oxygen, true);
        let without = cli_frame(&dir, &format!("sin-{oxygen}.png"), oxygen, false);
        let (original, _) = frame(&game_world(value, 1.3, false));
        assert!(with == without, "O2 {oxygen}: identico al juego sin flag");
        assert!(
            with == original,
            "O2 {oxygen}: identico a World::render original"
        );
    }
    for (oxygen, value) in [("0.25", 0.25), ("0.05", 0.05)] {
        let with = cli_frame(&dir, &format!("con-{oxygen}.png"), oxygen, true);
        let without = cli_frame(&dir, &format!("sin-{oxygen}.png"), oxygen, false);
        let (expected, lucid_stats) = frame(&game_world(value, 1.3, true));
        let (original, original_stats) = frame(&game_world(value, 1.3, false));
        assert!(
            with == expected,
            "O2 {oxygen}: Antena con el shader aceptado"
        );
        assert!(
            without == original,
            "O2 {oxygen}: sin flag, el juego de siempre"
        );
        let changed = with.iter().zip(&without).filter(|(a, b)| a != b).count();
        println!("O2 {oxygen}: {changed} pixeles distintos del juego sin flag");
        assert!(changed > 0, "O2 {oxygen}: la superficie lucida debe verse");
        assert_eq!(
            lucid_stats, original_stats,
            "misma geometria, LOD y descarte"
        );
        assert!(background(&with) == background(&without), "misma silueta");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn in_the_real_loop_oxygen_steps_change_only_antenas_shader_frame_by_frame() {
    // Mismas entradas al mundo con la superficie lucida y al mundo de siempre; la referencia es
    // el de siempre con solo `bodies[1].shader` cambiado.
    let mut lucid = game_world(1.0, 1.3, true);
    let mut plain = game_world(1.0, 1.3, false);
    let mut reference = game_world(1.0, 1.3, false);
    reference.system.bodies[1].shader = ShaderKind::LucidSurface;
    let input = ShipInput {
        thrust: 0.3,
        yaw: 0.2,
        ..ShipInput::default()
    };
    // Presets 1-4 (100/50/25/5 %) y pasos M/N, como en el bucle de ventana.
    let mut steps: Vec<(Option<f64>, bool, bool)> = vec![(None, false, false); 3];
    steps.extend([
        (Some(0.5), false, false),
        (None, false, false),
        (Some(0.25), false, false),
        (None, false, true),
        (None, false, true),
        (Some(0.05), false, false),
        (None, true, false),
        (Some(1.0), false, false),
    ]);
    let (mut below, mut at_or_above) = (0, 0);
    for (preset, more, less) in steps {
        for world in [&mut lucid, &mut plain, &mut reference] {
            if let Some(value) = preset {
                world.debug_oxygen01 = value;
            }
            world.debug_oxygen01 = step_debug_oxygen(world.debug_oxygen01, more, less);
            advance_frame(world, &input, 1.0 / 60.0);
        }
        let ((a, sa), (b, sb), (c, sc)) = (frame(&lucid), frame(&plain), frame(&reference));
        let oxygen = lucid.debug_oxygen01;
        assert!(a == c, "O2 {oxygen}: el cuadro es el de referencia");
        assert_eq!(sa, sc);
        assert_eq!(sa, sb, "misma geometria que el juego de siempre");
        if oxygen >= 0.5 {
            at_or_above += 1;
            assert!(a == b, "O2 {oxygen}: identico al juego");
        } else {
            below += 1;
            assert!(a != b, "O2 {oxygen}: Antena lucida");
        }
    }
    println!("cuadros: {below} bajo 50 %, {at_or_above} con 50 % o mas");
    assert!(below >= 4 && at_or_above >= 4, "{below} {at_or_above}");
    assert_eq!(lucid.system.bodies[1].shader, ShaderKind::LucidSurface);
}

#[test]
fn the_solar_composition_keeps_its_frame_at_high_oxygen_and_only_antena_changes_below_half() {
    for oxygen in [1.0, 0.5, 0.25, 0.05] {
        let (lucid_world, lucid_game) = solar_world(oxygen, 10.0, true);
        let (plain_world, plain_game) = solar_world(oxygen, 10.0, false);
        let (a, sa) = solar_frame(&lucid_world, &lucid_game);
        let (b, sb) = solar_frame(&plain_world, &plain_game);
        assert_eq!(sa, sb, "O2 {oxygen}: misma geometria");
        assert!(background(&a.pixels) == background(&b.pixels));
        assert_eq!(
            lucid_game.title(&lucid_world),
            plain_game.title(&plain_world)
        );
        if oxygen >= 0.5 {
            assert!(
                a.pixels == b.pixels,
                "O2 {oxygen}: la composicion de siempre"
            );
        } else {
            assert!(a.pixels != b.pixels, "O2 {oxygen}: Antena lucida");
        }
    }
}

#[test]
fn the_solar_library_entry_and_redundant_flags_give_the_expected_frame() {
    let dir = temp_dir("solar-entry");
    let (model, texture) = write_fixture(&dir);
    let output = dir.join("biblioteca.png");
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
    run_game_with_solar_and_lucid_surface(options).unwrap();
    let (world, game) = solar_world(0.25, 10.0, true);
    let (expected, _) = solar_frame(&world, &game);
    assert!(
        decode(&output).2 == expected.pixels,
        "entrada de biblioteca"
    );
    // CLI con los flags que la ruta solar ya implica: el mismo cuadro.
    let redundant = dir.join("redundantes.png");
    let o = std::ffi::OsStr::new;
    let result = run_cli(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
        o("--creature-solar-reveal"),
        o("--lucid-surface"),
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
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(decode(&redundant).2 == expected.pixels, "flags redundantes");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_plain_route_also_accepts_the_static_creature_and_its_reveal() {
    let dir = temp_dir("plain-creature");
    let (model, texture) = write_fixture(&dir);
    let output = dir.join("salida.png");
    let o = std::ffi::OsStr::new;
    let result = run_cli(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
        o("--creature-reveal"),
        o("--lucid-surface"),
        o("--debug-oxygen"),
        o("0.05"),
        o("--width"),
        o("320"),
        o("--height"),
        o("180"),
        o("--output"),
        output.as_os_str(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(decode(&output).0, 320);
    std::fs::remove_dir_all(&dir).unwrap();
}
