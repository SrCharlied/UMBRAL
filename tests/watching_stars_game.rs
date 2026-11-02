//! Parent tracer: approved observer02 must reach the real CLI and precede approved perception.
#[allow(dead_code)]
#[path = "../src/visual/watching_stars.rs"]
mod accepted_observers;
use std::{fs::File, io::BufReader, process::Command};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
    visual::perception_game::apply_lucid_perception,
};

#[test]
fn watching_stars_flag_uses_the_accepted_eyes_then_perception_on_the_real_world() {
    let path = std::env::temp_dir().join(format!(
        "umbral-watching-game03-tracer-{}.png",
        std::process::id()
    ));
    assert!(!path.exists(), "do not overwrite a previous artifact");
    let output = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--watching-stars",
            "--headless",
            "--width",
            "320",
            "--height",
            "180",
            "--debug-oxygen",
            "0.25",
            "--no-lod",
            "--output",
        ])
        .arg(&path)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "real game must accept --watching-stars and imply approved perception: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 320.0 / 180.0).unwrap();
    world.debug_oxygen01 = 0.25;
    world.toggles.lod = false;
    let mut frame = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.render(&mut frame);
    let eyes = accepted_observers::render_watching_stars(
        &frame,
        &world.camera,
        world.ship.position,
        world.debug_oxygen01,
    )
    .unwrap();
    assert!(
        eyes.pixels != frame.pixels,
        "the chosen frame must actually contain observing stars"
    );
    frame.pixels = eyes.pixels;
    apply_lucid_perception(&mut frame, &world).unwrap();
    let expected: Vec<u8> = frame
        .pixels
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
        expected.as_slice(),
        "real CLI must preserve accepted observer02 and compose eyes before the unchanged ship-focused post"
    );
    reader.finish().unwrap();
    std::fs::remove_file(path).unwrap();
}

// ---------------------------------------------------------------------------------------------
// Pruebas del worker (Bronya), integracion 03 de las estrellas observadoras. El prefijo de arriba
// es el tracer del padre, intacto. `accepted_observers` es el modulo congelado de preview02.
// ---------------------------------------------------------------------------------------------
use umbral::visual::{
    lucid_game::LucidSurfaceRequest, watching_stars_game::validate_watching_stars_flags,
};

#[test]
fn watching_stars_validator_admits_only_the_perception_scenes() {
    let check = |r: LucidSurfaceRequest| validate_watching_stars_flags(true, r);
    let everything = LucidSurfaceRequest {
        legacy: true,
        solar: false,
        eye_compare: true,
        distant_turn: true,
        horizontal_follow: true,
    };
    assert_eq!(validate_watching_stars_flags(false, everything), Ok(()));
    assert_eq!(check(LucidSurfaceRequest::default()), Ok(()));
    // En el Sol con ojo las demas combinaciones las decide su propio guard (que implica la
    // comparacion y el seguimiento 02).
    let solar = LucidSurfaceRequest {
        solar: true,
        eye_compare: true,
        horizontal_follow: true,
        ..LucidSurfaceRequest::default()
    };
    assert_eq!(check(solar), Ok(()));
    let legacy = Err("--legacy no admite --watching-stars".to_string());
    for solar in [false, true] {
        let request = LucidSurfaceRequest {
            legacy: true,
            solar,
            ..LucidSurfaceRequest::default()
        };
        assert_eq!(check(request), legacy, "solar {solar}");
    }
    for (request, flag) in [
        (
            LucidSurfaceRequest {
                eye_compare: true,
                ..LucidSurfaceRequest::default()
            },
            "--creature-eye-compare",
        ),
        (
            LucidSurfaceRequest {
                eye_compare: true,
                distant_turn: true,
                ..LucidSurfaceRequest::default()
            },
            "--creature-distant-turn",
        ),
        (
            LucidSurfaceRequest {
                eye_compare: true,
                horizontal_follow: true,
                ..LucidSurfaceRequest::default()
            },
            "--creature-horizontal-follow",
        ),
    ] {
        assert_eq!(
            check(request),
            Err(format!(
                "--watching-stars solo se combina con el juego normal o con --creature-solar-reveal: no admite {flag}"
            ))
        );
    }
}

use umbral::visual::watching_stars_game::render_game_frame;

const SMALL: (usize, usize) = (320, 180);

/// El arranque del juego normal (`--near` opcional) con el O2 mostrado puesto, como `app::run`.
fn game_world(near: Option<usize>, oxygen: f64) -> World {
    let mut world = World::new(
        load_model(&ship_model_path()).unwrap(),
        SMALL.0 as f64 / SMALL.1 as f64,
    )
    .unwrap();
    world.debug_oxygen01 = oxygen;
    if let Some(index) = near {
        world.place_near(index, 25.0).unwrap();
    }
    world
}

/// Profundidad observable: si cada pixel es cielo (>= 1) o tiene geometria delante.
fn sky_mask(fb: &Framebuffer) -> Vec<bool> {
    (0..fb.height as isize)
        .flat_map(|y| (0..fb.width as isize).map(move |x| (x, y)))
        .map(|(x, y)| fb.depth_passes(x, y, accepted_observers::SKY_DEPTH_PROBE))
        .collect()
}

#[test]
fn production_frame_is_the_scene_then_the_accepted_eyes_with_the_shown_oxygen() {
    for near in [None, Some(3)] {
        for oxygen in [1.0, 0.6, 0.5, 0.4, 0.25, 0.1, 0.0] {
            let world = game_world(near, oxygen);
            let mut scene = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
            let scene_stats = world.render(&mut scene);
            let eyes = accepted_observers::render_watching_stars(
                &scene,
                &world.camera,
                world.ship.position,
                oxygen,
            )
            .unwrap();
            let mut fb = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
            let frame = render_game_frame(&world, &mut fb, None, true).unwrap();
            assert!(
                fb.pixels == eyes.pixels,
                "{near:?} {oxygen}: production must be the scene plus the accepted preview02 eyes"
            );
            assert_eq!(
                format!("{:?}", frame.watching_stars),
                format!("{:?}", Some(&eyes.report)),
                "{near:?} {oxygen}: same eye metadata as preview02"
            );
            assert_eq!(
                frame.stats, scene_stats,
                "{near:?} {oxygen}: 3D stats are the scene's only (eyes add nothing)"
            );
            assert!(
                sky_mask(&fb) == sky_mask(&scene),
                "{near:?} {oxygen}: depth untouched"
            );
            let mut plain = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
            let without = render_game_frame(&world, &mut plain, None, false).unwrap();
            assert!(
                plain.pixels == scene.pixels
                    && without.watching_stars.is_none()
                    && without.stats == scene_stats,
                "{near:?} {oxygen}: without the flag the frame is World::render exactly"
            );
        }
    }
}

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Carpeta temporal propia de cada prueba (bajo el TMP del entorno; nunca en assets).
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "umbral-watching-game03-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// El binario real, siempre headless (nunca abre ventana).
fn run_cli(args: &[&OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

/// Pixeles `0xRRGGBB` de un PNG RGB de 8 bits.
fn decode(path: &Path) -> (usize, usize, Vec<u32>) {
    let mut reader = png::Decoder::new(BufReader::new(File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    let pixels = bytes[..info.buffer_size()]
        .chunks_exact(3)
        .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
        .collect();
    (info.width as usize, info.height as usize, pixels)
}

/// Cuadro del binario real con `args` (mas `--output` y 320x180), comprobando el exito.
fn cli_pixels(output: &Path, args: &[&str]) -> Vec<u32> {
    let mut all: Vec<&OsStr> = ["--width", "320", "--height", "180"]
        .iter()
        .chain(args)
        .map(OsStr::new)
        .collect();
    all.extend([OsStr::new("--output"), output.as_os_str()]);
    let result = run_cli(&all);
    assert!(
        result.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let (w, h, pixels) = decode(output);
    assert_eq!((w, h), SMALL);
    pixels
}

#[test]
fn cli_rejects_watching_stars_outside_its_scenes_before_any_io() {
    let dir = temp_dir("cli-guards");
    // Criatura INEXISTENTE: si algo intentara cargarla, el error seria de lectura, no del guard.
    let (model, texture) = (dir.join("no.obj"), dir.join("no.png"));
    let output = dir.join("salida.png");
    let o = OsStr::new;
    let creature = [
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
    ];
    let compare = [
        o("--creature-reveal-stages"),
        o("--creature-eye-follow"),
        o("--creature-eye-compare"),
    ];
    let watch = [o("--watching-stars")];
    let cases: Vec<(Vec<&OsStr>, &str)> = vec![
        (
            [&watch[..], &[o("--legacy")]].concat(),
            "--legacy no admite --watching-stars",
        ),
        (
            [&watch[..], &[o("--lucid-perception"), o("--legacy")]].concat(),
            "--legacy no admite --lucid-perception",
        ),
        (
            [&watch[..], &creature[..], &compare[..]].concat(),
            "--watching-stars solo se combina con el juego normal o con --creature-solar-reveal: no admite --creature-eye-compare",
        ),
        (
            [
                &watch[..],
                &creature[..],
                &compare[..],
                &[o("--creature-distant-turn")],
            ]
            .concat(),
            "no admite --creature-distant-turn",
        ),
        (
            [
                &watch[..],
                &creature[..],
                &compare[..],
                &[o("--creature-horizontal-follow")],
            ]
            .concat(),
            "no admite --creature-horizontal-follow",
        ),
        (
            [&watch[..], &[o("--creature-solar-reveal")]].concat(),
            "--creature-solar-reveal necesita --creature-model y --creature-texture",
        ),
        (
            [
                &watch[..],
                &creature[..],
                &[o("--creature-solar-reveal"), o("--near"), o("3")],
            ]
            .concat(),
            "--creature-solar-reveal y --near son incompatibles",
        ),
        (
            [&watch[..], &[o("--debug-oxygen"), o("1.5")]].concat(),
            "--debug-oxygen necesita un valor entre 0 y 1",
        ),
        (
            [&watch[..], &[o("--time"), o("-1")]].concat(),
            "--time necesita segundos >= 0",
        ),
        (
            [&watch[..], &[o("--width"), o("0")]].concat(),
            "--width debe ser mayor que cero",
        ),
        (
            [
                &watch[..],
                &[o("--width"), o("100000"), o("--height"), o("100000")],
            ]
            .concat(),
            "100000x100000 (maximo",
        ),
    ];
    for (args, expected) in cases {
        let mut all = args.clone();
        all.extend([o("--output"), output.as_os_str()]);
        let result = run_cli(&all);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success() && stderr.contains(expected),
            "{args:?}: expected the specific guard {expected:?}, got {stderr}"
        );
        assert!(!output.exists(), "{args:?}: nothing may be written");
        assert!(!model.exists() && !texture.exists());
    }
    // `.png` tambien sin criatura, antes de cargar la nave o crear el cuadro.
    let text = dir.join("salida.txt");
    let result = run_cli(&[o("--watching-stars"), o("--output"), text.as_os_str()]);
    assert!(
        !result.status.success()
            && String::from_utf8_lossy(&result.stderr).contains("--output debe terminar en .png"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!text.exists());
}

#[test]
fn cli_watching_stars_implies_the_accepted_perception_with_or_without_its_flag() {
    let dir = temp_dir("cli-implies");
    let png = |name: &str| dir.join(format!("{name}.png"));
    for oxygen in ["0.25", "0.4"] {
        let watch = cli_pixels(&png("w"), &["--watching-stars", "--debug-oxygen", oxygen]);
        let both = cli_pixels(
            &png("wp"),
            &[
                "--watching-stars",
                "--lucid-perception",
                "--debug-oxygen",
                oxygen,
            ],
        );
        let perception = cli_pixels(&png("p"), &["--lucid-perception", "--debug-oxygen", oxygen]);
        assert!(
            watch == both,
            "{oxygen}: the explicit perception flag is redundant"
        );
        assert!(
            watch != perception,
            "{oxygen}: the eyes must reach the real CLI frame"
        );
    }
    // Sin observadoras (O2 >= 65 %) y sin percepcion (>= 50 %) el cuadro es el de siempre.
    let watch = cli_pixels(&png("w100"), &["--watching-stars"]);
    let plain = cli_pixels(&png("plain"), &[]);
    assert!(watch == plain, "at 100% the frame is the usual game frame");
}

// --- Guards NACIDOS VERDES (born-green): pasan desde que se escribieron; su sensibilidad se
// demuestra con mutaciones acotadas en una copia desechable (worker-evidence/mutation_check.py).

use umbral::{
    app::{GameOptions, PerceptionRoute, advance_perception_frame, run_game_with_watching_stars},
    assets::obj::parse_obj,
    debug::overlay::{OverlayState, draw_overlay, overlay_lines},
    math::Vec3,
    renderer::texture::Texture,
    scene::ship::ShipInput,
    visual::{
        lucid_game::set_lucid_surface,
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, solar_creature_placement},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::{CreatureOptions, CreaturePlacement, StaticCreature},
        void_game::set_void_eye,
        watching_stars_game::apply_watching_stars,
    },
};

/// Opciones headless validas a 320x180 (nunca abren ventana).
fn options(output: PathBuf) -> GameOptions {
    GameOptions {
        width: SMALL.0,
        height: SMALL.1,
        headless: true,
        frame_limit: None,
        output,
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: None,
        creature_reveal: false,
        creature_reveal_stages: false,
        creature_eye_follow: false,
        debug_oxygen: 1.0,
    }
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

/// Cupula desechable con su region de ojo calibrada (la de las pruebas del Sol con ojo).
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

/// Modelo y textura DESECHABLES (la misma cupula) en `dir`: nunca los assets privados.
fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
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

#[test]
fn born_green_api_preflight_rejects_before_any_io_with_the_specific_guard() {
    let dir = temp_dir("api-guards");
    let (model, texture) = write_fixture(&dir);
    let fixture_bytes = (
        std::fs::read(&model).unwrap(),
        std::fs::read(&texture).unwrap(),
    );
    let output = dir.join("salida.png");
    let normal = PerceptionRoute::default();
    let solar = PerceptionRoute {
        solar: true,
        ..PerceptionRoute::default()
    };
    let creature = |near: bool| {
        Some(CreatureOptions {
            model: model.clone(),
            texture: texture.clone(),
            near,
        })
    };
    let missing = Some(CreatureOptions {
        model: dir.join("no.obj"),
        texture: dir.join("no.png"),
        near: false,
    });
    type Case<'a> = (
        Box<dyn Fn(&mut GameOptions) + 'a>,
        PerceptionRoute,
        &'static str,
    );
    let with = |m: &Path, t: &Path, near: bool| {
        Some(CreatureOptions {
            model: m.to_path_buf(),
            texture: t.to_path_buf(),
            near,
        })
    };
    let cases: Vec<Case> = vec![
        (
            Box::new(|o| o.width = 0),
            normal,
            "--width debe ser mayor que cero",
        ),
        (
            Box::new(|o| o.height = 0),
            normal,
            "--height debe ser mayor que cero",
        ),
        (
            Box::new(|o| (o.width, o.height) = (usize::MAX, 2)),
            normal,
            "(maximo",
        ),
        (
            Box::new(|o| (o.width, o.height) = (100_000, 100_000)),
            normal,
            "(maximo",
        ),
        (
            Box::new(|o| o.time = f64::NAN),
            normal,
            "--time necesita segundos >= 0",
        ),
        (
            Box::new(|o| o.time = -1.0),
            normal,
            "--time necesita segundos >= 0",
        ),
        (
            Box::new(|o| o.time = f64::INFINITY),
            normal,
            "--time necesita segundos >= 0",
        ),
        (
            Box::new(|o| o.debug_oxygen = 1.5),
            normal,
            "--debug-oxygen necesita un valor entre 0 y 1",
        ),
        (
            Box::new(|o| o.debug_oxygen = f64::NAN),
            normal,
            "--debug-oxygen necesita un valor entre 0 y 1",
        ),
        (
            Box::new(|o| o.output = o.output.with_extension("txt")),
            normal,
            "--output debe terminar en .png",
        ),
        // En el Sol el guard solar va antes: con criatura valida, el que falla es el de `.png`.
        (
            Box::new(|o| {
                (o.creature, o.output) = (with(&model, &texture, false), dir.join("salida.txt"))
            }),
            solar,
            "--output debe terminar en .png",
        ),
        (
            Box::new(|o| o.creature_reveal = true),
            normal,
            "--creature-reveal",
        ),
        (
            Box::new(|o| o.creature_eye_follow = true),
            normal,
            "--creature-eye-follow",
        ),
        (
            Box::new(|_| ()),
            solar,
            "--creature-solar-reveal necesita --creature-model y --creature-texture",
        ),
        (
            Box::new(|o| (o.creature, o.near) = (creature(false), Some(3))),
            solar,
            "--creature-solar-reveal y --near son incompatibles",
        ),
        (
            Box::new(|o| (o.creature, o.near) = (creature(true), Some(3))),
            normal,
            "--near y --near-creature son incompatibles",
        ),
        // Alias de --output con una entrada EXISTENTE (fixture temporal): el guard la protege.
        (
            Box::new(|o| (o.creature, o.output) = (with(&model, &texture, false), texture.clone())),
            normal,
            "coincide con la entrada",
        ),
        (
            Box::new(|o| {
                (o.creature, o.output) =
                    (with(&model, &texture, false), model.with_extension("png"))
            }),
            solar,
            "coincide con la entrada",
        ),
        // Criatura inexistente con un modo imposible: falla el guard, no la lectura.
        (
            Box::new(|o| (o.creature, o.creature_eye_follow) = (missing.clone(), true)),
            normal,
            "--creature-eye-follow",
        ),
    ];
    for (i, (edit, route, expected)) in cases.into_iter().enumerate() {
        let mut o = options(output.clone());
        edit(&mut o);
        let target = o.output.clone();
        let existed = target.exists();
        let error = run_game_with_watching_stars(o, route).unwrap_err();
        assert!(
            error.contains(expected),
            "case {i}: expected {expected:?}, got {error:?}"
        );
        assert!(
            !error.contains("No se pudo"),
            "case {i}: failed by IO, not by the guard: {error}"
        );
        assert!(
            !output.exists() && !output.with_extension("txt").exists(),
            "case {i}"
        );
        assert_eq!(target.exists(), existed, "case {i}");
        assert_eq!(
            (
                std::fs::read(&model).unwrap(),
                std::fs::read(&texture).unwrap()
            ),
            fixture_bytes,
            "case {i}: fixture bytes must not change"
        );
    }
}

/// Sol con ojo como lo arranca el juego (O2 y tiempo, superficie lucida y ojo del vacio si se
/// piden, `SolarGame::start`) y su cuadro con el helper de PRODUCCION, con o sin ojos.
fn solar_world(oxygen: f64, time: f64, lucid: bool, void: bool) -> (World, SolarGame) {
    let mut world = World::new(
        load_model(&ship_model_path()).unwrap(),
        SMALL.0 as f64 / SMALL.1 as f64,
    )
    .unwrap();
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
    (world, game)
}

/// La nave proyectada a mano con la camara real, en pixeles (como la usan los ojos), o `None`
/// si queda detras de la camara.
fn ship_px(world: &World) -> Option<[f64; 2]> {
    let c = world
        .camera
        .view_projection()
        .transform(world.ship.position.extend(1.0));
    (c.w > 0.0).then(|| {
        [
            (c.x / c.w * 0.5 + 0.5) * SMALL.0 as f64,
            (0.5 - c.y / c.w * 0.5) * SMALL.1 as f64,
        ]
    })
}

fn same_focus(a: Option<[f64; 2]>, b: Option<[f64; 2]>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
        (None, None) => true,
        _ => false,
    }
}

#[test]
fn born_green_solar_lucid_and_void_routes_get_the_accepted_eyes_under_the_post() {
    let dir = temp_dir("solar-routes");
    let (model, texture) = write_fixture(&dir);
    for (lucid, void) in [(false, false), (true, false), (false, true), (true, true)] {
        let (world, game) = solar_world(0.25, 1.3, lucid, void);
        let mut scene = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
        game.render(&world, &mut scene).unwrap();
        let eyes = accepted_observers::render_watching_stars(
            &scene,
            &world.camera,
            world.ship.position,
            0.25,
        )
        .unwrap();
        assert!(
            eyes.pixels != scene.pixels,
            "the Sun view must show observers"
        );
        assert!(
            same_focus(eyes.report.ship_focus_px, ship_px(&world)),
            "eyes look at the projected SHIP with the real camera, not the creature target"
        );
        let mut fb = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
        render_game_frame(&world, &mut fb, Some(&game), true).unwrap();
        assert!(
            fb.pixels == eyes.pixels,
            "{lucid} {void}: production solar frame"
        );
        apply_lucid_perception(&mut fb, &world).unwrap();
        let mut args = vec![
            "--watching-stars",
            "--creature-solar-reveal",
            "--debug-oxygen",
            "0.25",
            "--time",
            "1.3",
            "--creature-model",
            model.to_str().unwrap(),
            "--creature-texture",
            texture.to_str().unwrap(),
        ];
        if lucid {
            args.push("--lucid-surface");
        }
        if void {
            args.push("--void-eye");
        }
        let cli = cli_pixels(&dir.join(format!("solar-{lucid}-{void}.png")), &args);
        assert!(cli == fb.pixels, "{lucid} {void}: real CLI Sun route");
    }
    // Juego normal con superficie lucida y ojo del vacio, arrancando junto a Vigilia.
    for (lucid, void) in [(true, false), (false, true), (true, true)] {
        let mut world = game_world(None, 0.4);
        world.time = 1.3;
        if lucid {
            set_lucid_surface(&mut world, true).unwrap();
        }
        if void {
            set_void_eye(&mut world, true).unwrap();
        }
        world.place_near(3, 25.0).unwrap();
        let mut fb = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
        render_game_frame(&world, &mut fb, None, true).unwrap();
        apply_lucid_perception(&mut fb, &world).unwrap();
        let mut args = vec![
            "--watching-stars",
            "--near",
            "3",
            "--time",
            "1.3",
            "--debug-oxygen",
            "0.4",
        ];
        if lucid {
            args.push("--lucid-surface");
        }
        if void {
            args.push("--void-eye");
        }
        let cli = cli_pixels(&dir.join(format!("normal-{lucid}-{void}.png")), &args);
        assert!(cli == fb.pixels, "{lucid} {void}: real CLI normal route");
    }
}

/// Copia solo de los pixeles (`Framebuffer` no es `Clone` y su profundidad es privada); vale para
/// overlay y percepcion, que no usan la profundidad.
fn pixels_of(fb: &Framebuffer) -> Framebuffer {
    let mut copy = Framebuffer::new(fb.width, fb.height, SPACE_COLOR).unwrap();
    copy.pixels = fb.pixels.clone();
    copy
}

fn overlay_on(fb: &mut Framebuffer, world: &World, stats: &umbral::renderer::stats::RenderStats) {
    let state = OverlayState {
        culling: world.toggles.frustum_culling,
        lod: world.toggles.lod,
        backface: world.toggles.backface_culling,
        orbits_frozen: world.toggles.freeze_orbits,
    };
    let lines = overlay_lines(
        None,
        stats,
        &state,
        world.ship.speed,
        world.debug_oxygen01,
        world.time,
    );
    draw_overlay(fb, &lines);
}

#[test]
fn born_green_overlay_goes_after_the_eyes_and_only_the_post_changes_it() {
    let dir = temp_dir("overlay");
    for oxygen in [0.25, 0.0] {
        let world = game_world(None, oxygen);
        let mut eyed = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
        let frame = render_game_frame(&world, &mut eyed, None, true).unwrap();
        let mut plain = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
        render_game_frame(&world, &mut plain, None, false).unwrap();
        let mut over_plain = pixels_of(&plain);
        overlay_on(&mut over_plain, &world, &frame.stats);
        let mut expected = pixels_of(&eyed);
        overlay_on(&mut expected, &world, &frame.stats);
        // El overlay es una banda oscura SEMITRANSPARENTE con texto opaco: los ojos que quedan
        // debajo se ven oscurecidos (van antes), pero ningun glifo cambia por un ojo.
        const TEXT: u32 = 0xE8_F0F0;
        let (mut glyphs, mut eyes_under_band) = (0, 0);
        for i in 0..plain.pixels.len() {
            if over_plain.pixels[i] == TEXT && plain.pixels[i] != TEXT {
                glyphs += 1;
                assert_eq!(
                    expected.pixels[i], TEXT,
                    "{oxygen}: an eye over overlay text at {i}"
                );
            } else if over_plain.pixels[i] != plain.pixels[i]
                && expected.pixels[i] != over_plain.pixels[i]
            {
                eyes_under_band += 1;
            }
        }
        assert!(glyphs > 0);
        if oxygen > 0.0 {
            assert!(
                eyes_under_band > 0,
                "{oxygen}: eyes under the band are darkened by it"
            );
        }
        apply_lucid_perception(&mut expected, &world).unwrap();
        let o = format!("{oxygen}");
        let cli = cli_pixels(
            &dir.join(format!("overlay-{oxygen}.png")),
            &["--watching-stars", "--overlay", "--debug-oxygen", &o],
        );
        assert!(
            cli == expected.pixels,
            "{oxygen}: CLI = scene, eyes, overlay, approved post"
        );
        if oxygen == 0.0 {
            assert!(
                cli.iter().all(|&p| p == 0),
                "0% post erases everything, overlay included"
            );
        }
    }
}

#[test]
fn born_green_eyes_follow_the_shown_oxygen_of_the_real_manual_transition() {
    let mut world = game_world(None, 1.0);
    let mut oxygen = OxygenTransition::new(1.0).unwrap();
    let mut fb = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
    let still = ShipInput::default();
    let preset = |v: f64| OxygenKeys {
        preset: Some(v),
        ..OxygenKeys::default()
    };
    // (cuadro, teclas): caida a 25 %, reversion a 100 % a mitad, caida otra vez, F5 durante la
    // caida, R, N hasta 0 y recuperacion.
    let script: Vec<(usize, OxygenKeys)> = vec![
        (1, preset(0.25)),
        (20, preset(1.0)),
        (40, preset(0.25)),
        (
            80,
            OxygenKeys {
                preset: Some(0.05),
                decrease: true,
                ..OxygenKeys::default()
            },
        ),
        (140, preset(1.0)),
    ];
    let mut seen: Vec<(f64, Vec<usize>)> = Vec::new();
    for index in 0..=200usize {
        let keys = script
            .iter()
            .find(|(at, _)| *at == index)
            .map_or(OxygenKeys::default(), |(_, k)| *k);
        world.toggles.freeze_orbits = (50..70).contains(&index);
        if index == 60 {
            // R del juego normal: solo la nave; objetivo y mostrado se conservan.
            let before = (oxygen.target(), oxygen.shown(), world.debug_oxygen01);
            world.reset_ship();
            assert_eq!(
                (oxygen.target(), oxygen.shown(), world.debug_oxygen01),
                before
            );
        }
        let before = oxygen.shown();
        advance_perception_frame(&mut world, &mut oxygen, keys, &still, 1.0 / 12.0).unwrap();
        let shown = oxygen.shown();
        assert!(shown.is_finite() && (0.0..=1.0).contains(&shown));
        assert!(
            (shown - before).abs() <= 0.25 / 12.0 + 1e-12,
            "0.25 O2 per real second"
        );
        if (50..70).contains(&index) && before != oxygen.target() {
            assert!(
                shown != before,
                "F5 freezes orbits, not the oxygen transition"
            );
        }
        let frame = render_game_frame(&world, &mut fb, None, true).unwrap();
        let report = frame.watching_stars.unwrap();
        assert_eq!(
            report.oxygen01, world.debug_oxygen01,
            "{index}: eyes use the SHOWN O2"
        );
        assert_eq!(world.debug_oxygen01, shown, "{index}");
        // La poblacion es funcion del O2 mostrado (simetrica al bajar y al recuperar).
        if let Some((_, ids)) = seen.iter().find(|(o, _)| *o == shown) {
            assert_eq!(&report.active_ids, ids, "{index}: same O2, same observers");
        } else {
            seen.push((shown, report.active_ids.clone()));
        }
        if index == 120 {
            assert_eq!(
                (oxygen.target(), shown),
                (0.0, 0.0),
                "finite descent reaches 0 exactly"
            );
            let mut post = pixels_of(&fb);
            apply_lucid_perception(&mut post, &world).unwrap();
            assert!(post.pixels.iter().all(|&p| p == 0), "0% stays total black");
        }
    }
    assert_eq!(
        oxygen.shown(),
        1.0,
        "recovered exactly, without automatic consumption"
    );
    let mut plain = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
    world.render(&mut plain);
    assert!(
        fb.pixels == plain.pixels,
        "recovered at 100%: no observer is left"
    );
    assert!(seen.iter().any(|(o, ids)| *o <= 0.25 && ids.len() > 600));
}

#[test]
fn born_green_replay_controller_world_camera_eyes() {
    let mut world = game_world(None, 0.3);
    let mut oxygen = OxygenTransition::new(0.3).unwrap();
    let mut fb = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
    let turn = ShipInput {
        thrust: 1.0,
        yaw: 1.0,
        pitch: 0.5,
        ..ShipInput::default()
    };
    let (mut first_camera, mut previous) = (None, None::<Vec<u32>>);
    let mut responded = 0;
    for index in 0..36 {
        advance_perception_frame(
            &mut world,
            &mut oxygen,
            OxygenKeys::default(),
            &turn,
            1.0 / 12.0,
        )
        .unwrap();
        let report = render_game_frame(&world, &mut fb, None, true)
            .unwrap()
            .watching_stars
            .unwrap();
        assert!(
            same_focus(report.ship_focus_px, ship_px(&world)),
            "{index}: eyes look at the ship projected with this frame's real camera"
        );
        let focus = report.ship_focus_px.unwrap();
        let mut toward = 0;
        for e in &report.eyes {
            let d = [
                focus[0] - e.pixel.0 as f64 - 0.5,
                focus[1] - e.pixel.1 as f64 - 0.5,
            ];
            if d[0] * e.pupil_offset[0] + d[1] * e.pupil_offset[1] > 0.0 {
                toward += 1;
            }
        }
        assert!(
            !report.eyes.is_empty() && toward == report.eyes.len(),
            "{index}: every pupil toward the ship"
        );
        let camera = (world.camera.position, world.camera.target);
        let first = *first_camera.get_or_insert(camera);
        if index == 35 {
            assert!(first != camera, "the controller moved the real camera");
        }
        if previous.as_ref().is_some_and(|p| *p != fb.pixels) {
            responded += 1;
        }
        previous = Some(fb.pixels.clone());
    }
    assert!(
        responded >= 30,
        "the eye pixels respond to the moving camera: {responded}"
    );
}

#[test]
fn born_green_old_routes_never_get_eyes() {
    let dir = temp_dir("old-routes");
    for oxygen in ["0.25", "0.4"] {
        let o: f64 = oxygen.parse().unwrap();
        let world = game_world(None, o);
        let mut plain = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
        world.render(&mut plain);
        let without = cli_pixels(&dir.join("default.png"), &["--debug-oxygen", oxygen]);
        assert!(
            without == plain.pixels,
            "{oxygen}: default route is World::render exactly"
        );
        apply_lucid_perception(&mut plain, &world).unwrap();
        let perception = cli_pixels(
            &dir.join("perception.png"),
            &["--lucid-perception", "--debug-oxygen", oxygen],
        );
        assert!(
            perception == plain.pixels,
            "{oxygen}: old --lucid-perception has no eyes"
        );
    }
}

#[test]
fn born_green_apply_watching_stars_is_the_frozen_module_over_any_drawn_frame() {
    let world = game_world(Some(3), 0.25);
    let mut drawn = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
    world.render(&mut drawn);
    let frozen = accepted_observers::render_watching_stars(
        &drawn,
        &world.camera,
        world.ship.position,
        world.debug_oxygen01,
    )
    .unwrap();
    let mut fb = Framebuffer::new(SMALL.0, SMALL.1, SPACE_COLOR).unwrap();
    world.render(&mut fb);
    let report = apply_watching_stars(&mut fb, &world).unwrap();
    assert!(fb.pixels == frozen.pixels, "pixels of the frozen module");
    assert_eq!(format!("{report:?}"), format!("{:?}", frozen.report));
    assert!(sky_mask(&fb) == sky_mask(&drawn), "depth untouched");
}

// --- Cierre FIX01: preflight de `render_game_frame` antes de la escena (defecto reproducido por
// el supervisor en `parent-probes/frame_preflight.rs`). La primera prueba es esa regresion, copiada
// literal (sus `use` ya estan en este archivo).

#[test]
fn invalid_displayed_oxygen_is_rejected_before_drawing_the_scene() {
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 32.0 / 18.0).unwrap();
    world.debug_oxygen01 = f64::NAN;
    let mut frame = Framebuffer::new(32, 18, 0x12_3456).unwrap();
    let before = frame.pixels.clone();
    let result = render_game_frame(&world, &mut frame, None, true);
    assert!(result.is_err(), "invalid displayed oxygen must fail");
    assert!(
        frame.pixels == before,
        "reject invalid watching-star inputs BEFORE drawing/clearing the scene, not after partially replacing the caller's frame"
    );
}

/// Mundo valido 32x18 (como el probe del padre) con el O2 de los ojos ya en pantalla.
fn preflight_world() -> World {
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 32.0 / 18.0).unwrap();
    world.debug_oxygen01 = 0.25;
    world
}

/// Cuadro del llamador con fondo propio Y un patron distinto de ese fondo: cualquier limpieza
/// (`clear` vuelve al fondo) o dibujo de la escena lo cambiaria.
fn caller_frame() -> Framebuffer {
    let mut frame = Framebuffer::new(32, 18, 0x12_3456).unwrap();
    for (i, p) in frame.pixels.iter_mut().enumerate() {
        *p = (i as u32).wrapping_mul(0x9E37) & 0xFF_FFFF ^ 0x00_A5A5;
    }
    frame
}

/// Con `watching_stars = true`, `render_game_frame` debe devolver `Err` SIN tocar el cuadro
/// (pixeles ni dimensiones) y con el MISMO rechazo que el modulo congelado para esos inputs.
fn assert_rejected_untouched(world: &World, frame: &mut Framebuffer, case: &str) {
    let (before, shape) = (frame.pixels.clone(), (frame.width, frame.height));
    let result = render_game_frame(world, frame, None, true).map(|_| ());
    assert!(result.is_err(), "{case}: must be rejected");
    assert!(
        frame.pixels == before && (frame.width, frame.height) == shape,
        "{case}: rejected AFTER touching the caller's frame (scene drawn or cleared first)"
    );
    let frozen = accepted_observers::render_watching_stars(
        frame,
        &world.camera,
        world.ship.position,
        world.debug_oxygen01,
    )
    .map(|_| ());
    assert_eq!(
        result, frozen,
        "{case}: same rejection as the frozen module"
    );
}

/// Una alteracion nombrada de un input (mundo o cuadro) para los casos de rechazo.
type Edit<T> = Box<dyn Fn(&mut T)>;

#[test]
fn watch_frame_rejects_invalid_oxygen_before_the_scene() {
    for oxygen in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        let mut world = preflight_world();
        world.debug_oxygen01 = oxygen;
        assert_rejected_untouched(&world, &mut caller_frame(), &format!("O2 {oxygen}"));
    }
}

#[test]
fn watch_frame_rejects_a_non_finite_ship_before_the_scene() {
    for (axis, value) in [(0, f64::NAN), (1, f64::INFINITY), (2, f64::NEG_INFINITY)] {
        let mut world = preflight_world();
        match axis {
            0 => world.ship.position.x = value,
            1 => world.ship.position.y = value,
            _ => world.ship.position.z = value,
        }
        assert_rejected_untouched(
            &world,
            &mut caller_frame(),
            &format!("ship axis {axis} {value}"),
        );
    }
}

#[test]
fn watch_frame_rejects_a_degenerate_or_non_finite_camera_before_the_scene() {
    let cases: Vec<(&str, Edit<World>)> = vec![
        ("position NaN", Box::new(|w| w.camera.position.x = f64::NAN)),
        (
            "target infinite",
            Box::new(|w| w.camera.target.y = f64::INFINITY),
        ),
        ("up NaN", Box::new(|w| w.camera.up.z = f64::NAN)),
        (
            "target at the position",
            Box::new(|w| w.camera.target = w.camera.position),
        ),
        (
            "zero up",
            Box::new(|w| w.camera.up = Vec3::new(0.0, 0.0, 0.0)),
        ),
        // Sin caso de «matrices no finitas»: con posicion, objetivo y up finitos que pasan los
        // guards anteriores no es alcanzable (`normalized()` da cero si la longitud desborda y la
        // proyeccion es privada y validada por `Camera::new`). El preflight conserva ese guard por
        // equivalencia con el modulo congelado.
    ];
    for (case, edit) in cases {
        let mut world = preflight_world();
        edit(&mut world);
        assert_rejected_untouched(&world, &mut caller_frame(), case);
    }
}

#[test]
fn watch_frame_rejects_an_inconsistent_public_frame_shape_before_the_scene() {
    use umbral::renderer::framebuffer::MAX_PIXELS;
    // Cuadros PEQUENOS con campos publicos alterados (sin reservas gigantes). El primero va antes
    // porque la escena no se sale de rango con el y el fallo previo es por la asercion.
    let cases: Vec<(&str, Edit<Framebuffer>)> = vec![
        ("one extra pixel", Box::new(|f| f.pixels.push(0))),
        (
            "one missing pixel",
            Box::new(|f| {
                f.pixels.pop();
            }),
        ),
        ("zero width", Box::new(|f| f.width = 0)),
        ("zero height", Box::new(|f| f.height = 0)),
        (
            "width that does not match the pixels",
            Box::new(|f| f.width = 31),
        ),
        (
            "width x height overflows",
            Box::new(|f| f.width = usize::MAX),
        ),
        (
            "more than MAX_PIXELS",
            Box::new(|f| (f.width, f.height) = (MAX_PIXELS + 1, 1)),
        ),
    ];
    for (case, edit) in cases {
        let mut frame = caller_frame();
        edit(&mut frame);
        assert_rejected_untouched(&preflight_world(), &mut frame, case);
    }
}

#[test]
fn born_green_preflight_keeps_the_valid_and_flagless_paths() {
    // Sin el flag no hay preflight: el cuadro es `World::render` como antes, aun con inputs que
    // los ojos rechazarian.
    for case in ["O2 NaN", "ship NaN"] {
        let mut world = preflight_world();
        if case == "O2 NaN" {
            world.debug_oxygen01 = f64::NAN;
        } else {
            world.ship.position.x = f64::NAN;
        }
        let (mut scene, mut fb) = (caller_frame(), caller_frame());
        let stats = world.render(&mut scene);
        let frame = render_game_frame(&world, &mut fb, None, false).unwrap();
        assert!(
            fb.pixels == scene.pixels && frame.stats == stats && frame.watching_stars.is_none(),
            "{case}: without the flag the frame is World::render exactly"
        );
    }
    // Con el flag, los limites validos (0 y 1 incluidos) pasan y son exactamente el modulo.
    for oxygen in [0.0, 0.5, 1.0] {
        let mut world = preflight_world();
        world.debug_oxygen01 = oxygen;
        let mut scene = caller_frame();
        world.render(&mut scene);
        let frozen = accepted_observers::render_watching_stars(
            &scene,
            &world.camera,
            world.ship.position,
            oxygen,
        )
        .unwrap();
        let mut fb = caller_frame();
        let frame = render_game_frame(&world, &mut fb, None, true).unwrap();
        assert!(fb.pixels == frozen.pixels, "{oxygen}: valid path unchanged");
        assert_eq!(
            format!("{:?}", frame.watching_stars),
            format!("{:?}", Some(&frozen.report)),
            "{oxygen}"
        );
    }
}
