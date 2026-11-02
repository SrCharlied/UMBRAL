//! Supervisor tracer: approved perception02 through the real application's CLI.
use std::{fs::File, io::BufReader, process::Command};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
};
#[path = "../src/visual/lucid_perception_variant.rs"]
mod approved;

#[test]
fn perception_flag_processes_the_real_world_with_the_approved_ship_focused_variant() {
    let path = std::env::temp_dir().join(format!(
        "umbral_perception_tracer_{}.png",
        std::process::id()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--lucid-perception",
            "--headless",
            "--width",
            "320",
            "--height",
            "180",
            "--near",
            "3",
            "--time",
            "1.3",
            "--debug-oxygen",
            "0.05",
            "--no-lod",
            "--output",
        ])
        .arg(&path)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "real game must accept --lucid-perception: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 320.0 / 180.0).unwrap();
    world.time = 1.3;
    world.debug_oxygen01 = 0.05;
    world.toggles.lod = false;
    world.place_near(3, 25.0).unwrap();
    let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.render(&mut frame);
    let clip = world
        .camera
        .view_projection()
        .transform(world.ship.position.extend(1.0));
    let focus = [clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5];
    let expected = approved::lucid_perception_variant(&frame, 0.05, 1.3, focus, false).unwrap();
    assert_ne!(expected, frame.pixels);
    let rgb: Vec<u8> = expected
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect();
    let mut reader = png::Decoder::new(BufReader::new(File::open(&path).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (320, 180));
    assert_eq!(
        &bytes[..info.buffer_size()],
        rgb.as_slice(),
        "headless must use the unchanged approved postprocess and the actual ship projection"
    );
    std::fs::remove_file(path).unwrap();
}

use umbral::{
    math::Vec3,
    visual::perception_game::{ShipFocus, apply_lucid_perception, ship_focus},
};

/// El arranque del tracer (`--near 3`, tiempo 1.3, sin LOD) con el O2 dado.
fn near_world(width: usize, height: usize, oxygen: f64) -> World {
    let model = load_model(&ship_model_path()).unwrap();
    let mut world = World::new(model, width as f64 / height as f64).unwrap();
    world.time = 1.3;
    world.debug_oxygen01 = oxygen;
    world.toggles.lod = false;
    world.place_near(3, 25.0).unwrap();
    world
}

/// La nave proyectada a mano con la camara del mundo, como en el tracer.
fn projected(world: &World) -> [f64; 2] {
    let clip = world
        .camera
        .view_projection()
        .transform(world.ship.position.extend(1.0));
    [clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5]
}

/// Mueve el punto de mira de la camara a su derecha (`fraction` de la distancia al objetivo):
/// la nave queda a la izquierda de la pantalla sin moverla.
fn look_right(world: &mut World, fraction: f64) {
    let camera = world.camera;
    let distance = (camera.target - camera.position).length();
    let right = camera.forward().cross(camera.up).normalized();
    world.camera.target = camera.target + right * (fraction * distance);
}

#[test]
fn the_focus_is_the_ship_projected_with_the_real_camera_not_the_screen_center() {
    let mut world = near_world(320, 180, 0.05);
    let start = ship_focus(&world);
    assert!(start.projected, "{start:?}");
    assert_eq!(start.uv, projected(&world));
    assert!(start.uv[1] > 0.55, "la nave va bajo el centro: {start:?}");
    look_right(&mut world, 0.2);
    let moved = ship_focus(&world);
    assert_eq!(moved.uv, projected(&world));
    assert!(
        moved.uv[0] < 0.45,
        "la nave queda a la izquierda: {moved:?}"
    );
}

#[test]
fn the_postprocess_tunnel_follows_an_off_center_ship() {
    let mut world = near_world(320, 180, 0.05);
    look_right(&mut world, 0.2);
    let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.render(&mut frame);
    let focus = ship_focus(&world).uv;
    let expected = approved::lucid_perception_variant(&frame, 0.05, 1.3, focus, false).unwrap();
    let centered =
        approved::lucid_perception_variant(&frame, 0.05, 1.3, [0.5, 0.5], false).unwrap();
    assert!(
        expected != centered,
        "el foco debe ser la nave, no el centro"
    );
    apply_lucid_perception(&mut frame, &world).unwrap();
    assert!(
        frame.pixels == expected,
        "postproceso con el foco de la nave"
    );
}

#[test]
fn without_a_valid_projection_the_focus_is_the_screen_center_and_says_so() {
    let mut world = near_world(320, 180, 0.05);
    // Nave delante pero fuera de la pantalla: el foco queda en el borde, dentro de [0, 1]^2.
    look_right(&mut world, 3.0);
    let outside = ship_focus(&world);
    assert!(outside.projected, "{outside:?}");
    assert!(projected(&world)[0] < 0.0, "{:?}", projected(&world));
    assert_eq!(outside.uv[0], 0.0, "{outside:?}");
    assert!((0.0..=1.0).contains(&outside.uv[1]), "{outside:?}");
    let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.render(&mut frame);
    apply_lucid_perception(&mut frame, &world).unwrap();
    // Camara mirando hacia atras: la nave queda detras (w <= 0), sin proyeccion valida.
    let mut world = near_world(320, 180, 0.05);
    let camera = world.camera;
    world.camera.target = camera.position + (camera.position - camera.target);
    let clip = world
        .camera
        .view_projection()
        .transform(world.ship.position.extend(1.0));
    assert!(clip.w <= 0.0, "{clip:?}");
    let center = ShipFocus {
        uv: [0.5, 0.5],
        projected: false,
    };
    assert_eq!(ship_focus(&world), center);
    let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.render(&mut frame);
    apply_lucid_perception(&mut frame, &world).unwrap();
    // Una nave con posicion no finita tampoco proyecta.
    world.ship.position = Vec3::new(f64::NAN, 0.0, 0.0);
    assert_eq!(ship_focus(&world), center);
}

use umbral::{
    assets::obj::parse_obj,
    renderer::texture::Texture,
    visual::{
        lucid_game::set_lucid_surface,
        solar_game::{SolarGame, solar_creature_placement},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::{CreaturePlacement, StaticCreature},
        void_game::set_void_eye,
    },
};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "umbral-perception-game-{name}-{}",
        std::process::id()
    ));
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

/// Cuadro del binario real con `args` (mas `--output`), comprobando el exito y el tamano.
fn cli_pixels(output: &std::path::Path, args: &[&str], size: (usize, usize)) -> Vec<u32> {
    let mut all: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
    all.extend([std::ffi::OsStr::new("--output"), output.as_os_str()]);
    let result = run_cli(&all);
    assert!(
        result.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let (w, h, pixels) = decode(output);
    assert_eq!((w, h), size);
    pixels
}

/// Lo que debe salir: el modulo aprobado sobre el cuadro ya dibujado, con el O2 y el reloj del
/// mundo y la nave proyectada a mano (dentro de la pantalla) como foco.
fn approved_over(frame: &Framebuffer, world: &World) -> Vec<u32> {
    let focus = projected(world);
    assert!(focus.iter().all(|v| (0.0..=1.0).contains(v)), "{focus:?}");
    approved::lucid_perception_variant(frame, world.debug_oxygen01, world.time, focus, false)
        .unwrap()
}

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

/// Cupula desechable con su region de ojo calibrada (como en las pruebas del Sol con ojo).
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

/// El arranque del Sol con ojo como lo hace el juego (O2 y tiempo puestos, superficie lucida y
/// ojo del vacio si se piden, `SolarGame::start`) y su cuadro ya dibujado.
fn solar_frame(oxygen: f64, time: f64, lucid: bool, void: bool) -> (World, Framebuffer) {
    let model = load_model(&ship_model_path()).unwrap();
    let mut world = World::new(model, 320.0 / 180.0).unwrap();
    world.time = time;
    world.debug_oxygen01 = oxygen;
    if lucid {
        set_lucid_surface(&mut world, true).unwrap();
    }
    if void {
        set_void_eye(&mut world, true).unwrap();
    }
    let placement: CreaturePlacement = solar_creature_placement(&world).unwrap();
    let texture = Texture::from_rgb8(2, 2, TEXELS.to_vec()).unwrap();
    let creature =
        StaticCreature::new(parse_obj(&calibrated_source()).unwrap(), texture, placement).unwrap();
    let game = SolarGame::start(&mut world, creature).unwrap();
    let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    game.render(&world, &mut frame).unwrap();
    (world, frame)
}

#[test]
fn the_normal_routes_with_lucid_surface_and_void_eye_get_the_last_step_over_their_own_scene() {
    let dir = temp_dir("normal-routes");
    // Antena (1) y Vigilia (3) en cuadro, para que cada flag cambie la escena de verdad.
    for (lucid, void, near) in [
        (true, false, "1"),
        (false, true, "3"),
        (true, true, "1"),
        (true, true, "3"),
    ] {
        let mut args = vec![
            "--lucid-perception",
            "--width",
            "320",
            "--height",
            "180",
            "--near",
            near,
            "--time",
            "1.3",
            "--debug-oxygen",
            "0.25",
            "--no-lod",
        ];
        if lucid {
            args.push("--lucid-surface");
        }
        if void {
            args.push("--void-eye");
        }
        let output = dir.join(format!("normal-{lucid}-{void}-{near}.png"));
        let pixels = cli_pixels(&output, &args, (320, 180));
        let model = load_model(&ship_model_path()).unwrap();
        let mut world = World::new(model, 320.0 / 180.0).unwrap();
        world.time = 1.3;
        world.debug_oxygen01 = 0.25;
        world.toggles.lod = false;
        world.place_near(near.parse().unwrap(), 25.0).unwrap();
        let mut plain = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        world.render(&mut plain);
        if lucid {
            set_lucid_surface(&mut world, true).unwrap();
        }
        if void {
            set_void_eye(&mut world, true).unwrap();
        }
        let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        world.render(&mut frame);
        assert!(
            frame.pixels != plain.pixels,
            "la escena {near} debe cambiar"
        );
        let expected = approved_over(&frame, &world);
        assert!(pixels == expected, "lucid {lucid} void {void} near {near}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_solar_routes_get_the_last_step_over_the_solar_composition() {
    let dir = temp_dir("solar-routes");
    let (model, texture) = write_fixture(&dir);
    let (model, texture) = (model.to_str().unwrap(), texture.to_str().unwrap());
    for (lucid, void) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut args = vec![
            "--lucid-perception",
            "--creature-model",
            model,
            "--creature-texture",
            texture,
            "--creature-solar-reveal",
            "--width",
            "320",
            "--height",
            "180",
            "--time",
            "10",
            "--debug-oxygen",
            "0.25",
        ];
        if lucid {
            args.push("--lucid-surface");
        }
        if void {
            args.push("--void-eye");
        }
        let output = dir.join(format!("solar-{lucid}-{void}.png"));
        let pixels = cli_pixels(&output, &args, (320, 180));
        let (world, frame) = solar_frame(0.25, 10.0, lucid, void);
        let expected = approved_over(&frame, &world);
        assert!(pixels == expected, "solar lucid {lucid} void {void}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

use umbral::{
    app::{GameOptions, PerceptionRoute, run_game_with_lucid_perception},
    visual::{
        lucid_game::LucidSurfaceRequest, perception_game::validate_lucid_perception_flags,
        static_creature::CreatureOptions,
    },
};

#[test]
fn the_perception_flag_validator_accepts_only_its_routes() {
    let base = LucidSurfaceRequest::default();
    assert!(
        validate_lucid_perception_flags(
            false,
            LucidSurfaceRequest {
                legacy: true,
                ..base
            }
        )
        .is_ok()
    );
    assert!(validate_lucid_perception_flags(true, base).is_ok());
    let solar = LucidSurfaceRequest {
        solar: true,
        eye_compare: true,
        horizontal_follow: true,
        ..base
    };
    assert!(
        validate_lucid_perception_flags(true, solar).is_ok(),
        "el Sol implica sus flags"
    );
    for (request, needle) in [
        (
            LucidSurfaceRequest {
                legacy: true,
                ..base
            },
            "--legacy no admite --lucid-perception",
        ),
        (
            LucidSurfaceRequest {
                eye_compare: true,
                ..base
            },
            "--creature-eye-compare",
        ),
        (
            LucidSurfaceRequest {
                distant_turn: true,
                eye_compare: true,
                ..base
            },
            "--creature-distant-turn",
        ),
        (
            LucidSurfaceRequest {
                horizontal_follow: true,
                eye_compare: true,
                ..base
            },
            "--creature-horizontal-follow",
        ),
        (
            LucidSurfaceRequest {
                legacy: true,
                solar: true,
                ..base
            },
            "--legacy no admite --lucid-perception",
        ),
    ] {
        let error = validate_lucid_perception_flags(true, request).unwrap_err();
        assert!(error.contains(needle), "{request:?}: {error}");
    }
}

#[test]
fn the_command_line_rejects_perception_outside_its_routes_before_any_io() {
    let dir = temp_dir("cli-guards");
    let (model, texture) = (dir.join("no.obj"), dir.join("no.png"));
    let (output, text_output) = (dir.join("salida.png"), dir.join("salida.txt"));
    let o = std::ffi::OsStr::new;
    let creature = [
        o("--lucid-perception"),
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
        o("--output"),
        output.as_os_str(),
    ];
    let compare = [
        o("--creature-reveal-stages"),
        o("--creature-eye-follow"),
        o("--creature-eye-compare"),
    ];
    let plain = [o("--lucid-perception"), o("--output"), output.as_os_str()];
    let cases: Vec<(Vec<&std::ffi::OsStr>, &str)> = vec![
        (
            [&plain[..], &[o("--legacy")]].concat(),
            "--legacy no admite --lucid-perception",
        ),
        (
            [
                &plain[..],
                &[o("--legacy"), o("--void-eye"), o("--lucid-surface")],
            ]
            .concat(),
            "--legacy no admite",
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
        (
            [
                &creature[..],
                &[o("--creature-solar-reveal"), o("--creature-distant-turn")],
            ]
            .concat(),
            "--creature-distant-turn",
        ),
        (
            vec![
                o("--lucid-perception"),
                o("--output"),
                text_output.as_os_str(),
            ],
            "--output debe terminar en .png",
        ),
        (
            [
                &plain[..],
                &[o("--width"), o("100000"), o("--height"), o("100000")],
            ]
            .concat(),
            "maximo",
        ),
    ];
    for (args, needle) in cases {
        let result = run_cli(&args);
        let (stdout, stderr) = (
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr),
        );
        assert!(!result.status.success(), "{args:?} debia fallar");
        assert!(stderr.contains(needle), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("no.obj") && !stderr.contains("no.png"),
            "el error debe llegar antes de leer los assets: {stderr}"
        );
        assert!(
            !stdout.contains("Modelo:"),
            "ni siquiera carga la nave: {stdout}"
        );
        assert!(
            !output.exists() && !text_output.exists(),
            "no escribe: {args:?}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

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

#[test]
fn the_library_entry_validates_everything_before_any_io_on_every_route() {
    let dir = temp_dir("library-guards");
    let output = dir.join("salida.png");
    // Assets desechables existentes (nunca los reales): un alias de --output no debe tocarlos.
    let (dummy_model, dummy_texture) = (dir.join("desechable.obj"), dir.join("desechable.png"));
    std::fs::write(&dummy_model, b"# modelo desechable\n").unwrap();
    std::fs::write(&dummy_texture, b"textura desechable").unwrap();
    let creature = |near: bool| {
        Some(CreatureOptions {
            model: dummy_model.clone(),
            texture: dummy_texture.clone(),
            near,
        })
    };
    for solar in [false, true] {
        for (lucid, void) in [(false, false), (true, false), (false, true), (true, true)] {
            let route = PerceptionRoute { lucid, void, solar };
            // Opciones validas de la ruta salvo el defecto indicado.
            let valid = |options: GameOptions| {
                if solar {
                    GameOptions {
                        near: None,
                        creature: creature(false),
                        ..options
                    }
                } else {
                    options
                }
            };
            let mut cases: Vec<(GameOptions, &str)> = vec![
                (
                    valid(GameOptions {
                        width: 0,
                        ..library_options(&output)
                    }),
                    "--width",
                ),
                (
                    valid(GameOptions {
                        height: 0,
                        ..library_options(&output)
                    }),
                    "--height",
                ),
                (
                    valid(GameOptions {
                        width: usize::MAX,
                        height: 2,
                        ..library_options(&output)
                    }),
                    "maximo",
                ),
                (
                    valid(GameOptions {
                        time: f64::NAN,
                        ..library_options(&output)
                    }),
                    "--time",
                ),
                (
                    valid(GameOptions {
                        time: -1.0,
                        ..library_options(&output)
                    }),
                    "--time",
                ),
                (
                    valid(GameOptions {
                        debug_oxygen: f64::NAN,
                        ..library_options(&output)
                    }),
                    "--debug-oxygen",
                ),
                (
                    valid(GameOptions {
                        debug_oxygen: 1.5,
                        ..library_options(&output)
                    }),
                    "--debug-oxygen",
                ),
                (
                    valid(GameOptions {
                        output: dir.join("salida.txt"),
                        ..library_options(&output)
                    }),
                    "--output debe terminar en .png",
                ),
                (
                    GameOptions {
                        output: dummy_texture.clone(),
                        ..valid(GameOptions {
                            creature: creature(false),
                            ..library_options(&output)
                        })
                    },
                    "coincide con la entrada",
                ),
            ];
            if solar {
                cases.push((
                    GameOptions {
                        creature: None,
                        ..valid(library_options(&output))
                    },
                    "--creature-model",
                ));
                cases.push((
                    GameOptions {
                        near: Some(3),
                        ..valid(library_options(&output))
                    },
                    "--near",
                ));
                cases.push((
                    GameOptions {
                        creature_reveal: true,
                        ..valid(library_options(&output))
                    },
                    "--creature-reveal",
                ));
            } else {
                cases.push((
                    GameOptions {
                        creature_reveal: true,
                        ..library_options(&output)
                    },
                    "--creature-reveal necesita",
                ));
                cases.push((
                    GameOptions {
                        creature: creature(true),
                        ..library_options(&output)
                    },
                    "--near y --near-creature",
                ));
                cases.push((
                    GameOptions {
                        creature_eye_follow: true,
                        creature: creature(false),
                        ..library_options(&output)
                    },
                    "--creature-eye-follow",
                ));
            }
            for (options, needle) in cases {
                let described = format!(
                    "{route:?} {}x{} t {} O2 {} {}",
                    options.width,
                    options.height,
                    options.time,
                    options.debug_oxygen,
                    options.output.display()
                );
                let error = run_game_with_lucid_perception(options, route).unwrap_err();
                assert!(error.contains(needle), "{described}: {error}");
                assert!(
                    !output.exists() && !dir.join("salida.txt").exists(),
                    "{described}"
                );
                assert_eq!(
                    std::fs::read(&dummy_model).unwrap(),
                    b"# modelo desechable\n"
                );
                assert_eq!(
                    std::fs::read(&dummy_texture).unwrap(),
                    b"textura desechable"
                );
            }
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

// Guards de preservacion (nacidos GREEN; su sensibilidad se prueba con mutaciones en copia).

/// Argumentos del arranque del tracer con tamano y O2 dados.
fn tracer_args(
    width: &'static str,
    height: &'static str,
    oxygen: &'static str,
) -> Vec<&'static str> {
    vec![
        "--width",
        width,
        "--height",
        height,
        "--near",
        "3",
        "--time",
        "1.3",
        "--debug-oxygen",
        oxygen,
        "--no-lod",
    ]
}

#[test]
fn the_cli_export_equals_the_approved_module_at_every_level_and_both_resolutions() {
    let dir = temp_dir("export-levels");
    for (width, height) in [("320", "180"), ("1280", "720")] {
        let size: (usize, usize) = (width.parse().unwrap(), height.parse().unwrap());
        for oxygen in ["1", "0.5", "0.25", "0.1", "0.05", "0.01", "0"] {
            let mut args = vec!["--lucid-perception"];
            args.extend(tracer_args(width, height, oxygen));
            let output = dir.join(format!("export-{width}-{oxygen}.png"));
            let pixels = cli_pixels(&output, &args, size);
            let world = near_world(size.0, size.1, oxygen.parse().unwrap());
            let mut frame = Framebuffer::new(size.0, size.1, SPACE_COLOR).unwrap();
            world.render(&mut frame);
            let expected = approved_over(&frame, &world);
            assert!(pixels == expected, "{width}x{height} O2 {oxygen}");
            let black = pixels.iter().filter(|&&p| p == 0).count();
            match oxygen {
                "1" | "0.5" => assert!(pixels == frame.pixels, "identidad con O2 {oxygen}"),
                "0" => assert_eq!(black, pixels.len(), "negro total solo con O2 0"),
                _ => assert!(black < pixels.len(), "O2 {oxygen}: aun se ve la nave"),
            }
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn without_the_flag_the_game_is_exactly_the_same_as_always() {
    let dir = temp_dir("no-flag");
    for oxygen in ["1", "0.25", "0.05", "0"] {
        let output = dir.join(format!("sin-flag-{oxygen}.png"));
        let pixels = cli_pixels(&output, &tracer_args("320", "180", oxygen), (320, 180));
        let world = near_world(320, 180, oxygen.parse().unwrap());
        let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        world.render(&mut frame);
        assert!(
            pixels == frame.pixels,
            "sin flag, O2 {oxygen}: el cuadro de siempre"
        );
    }
    // Sin el flag un --output no .png sigue permitido sin criatura, como siempre.
    let text = dir.join("sin-flag.txt");
    let o = std::ffi::OsStr::new;
    let result = run_cli(&[o("--output"), text.as_os_str()]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(text.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_postprocess_is_the_last_color_step_after_the_overlay() {
    let dir = temp_dir("overlay");
    for oxygen in ["0.25", "0"] {
        let mut args = tracer_args("320", "180", oxygen);
        args.push("--overlay");
        let plain = cli_pixels(
            &dir.join(format!("overlay-{oxygen}.png")),
            &args,
            (320, 180),
        );
        args.push("--lucid-perception");
        let pixels = cli_pixels(
            &dir.join(format!("overlay-p-{oxygen}.png")),
            &args,
            (320, 180),
        );
        // Fuente = el cuadro de siempre CON overlay; el foco y el O2 son los del mundo.
        let world = near_world(320, 180, oxygen.parse().unwrap());
        let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        world.render(&mut frame);
        assert!(plain != frame.pixels, "el overlay dibuja texto");
        frame.pixels = plain;
        let expected = approved_over(&frame, &world);
        assert!(
            pixels == expected,
            "O2 {oxygen}: postproceso tras el overlay"
        );
        if oxygen == "0" {
            assert!(
                pixels.iter().all(|&p| p == 0),
                "con O2 0 no queda texto brillante"
            );
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
