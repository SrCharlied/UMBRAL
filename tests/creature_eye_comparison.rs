//! Comparacion A/B del objetivo de la mirada (nave o camara): `World`, diagnostico, panel y
//! binario. Fixtures sinteticos (sin el asset).

use umbral::{
    assets::{
        obj::{Model, load_model, parse_obj},
        ship_model_path,
    },
    math::Vec3,
    renderer::{framebuffer::Framebuffer, stats::RenderStats, texture::Texture},
    scene::world::{SPACE_COLOR, World},
    visual::{
        creature_eye_comparison::*,
        creature_eye_follow::{EyeFollow, EyeFollowSettings, EyeSkin, GazeAngles},
        staged_creature_reveal::{EyeRegion, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const W: usize = 320;
const H: usize = 180;

/// Cupula calibrable de las pruebas de la mirada (radio 1, apice +Z, anillos de 10 grados,
/// 16 segmentos): con `dome_region` las bandas 0-4 son ojo y las 5-7 cuerpo.
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

fn offset_dome_source() -> String {
    dome_source() + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n"
}

/// Cupula con un triangulo de cuerpo desplazado: bounds descentrados como en el asset real y
/// mirada no neutral en el arranque `--near-creature`.
fn offset_dome() -> Model {
    parse_obj(&offset_dome_source()).unwrap()
}

fn dome_region() -> EyeRegion {
    EyeRegion::new(Vec3::Z, 0.85).unwrap()
}

fn texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160],
    )
    .unwrap()
}

fn creature(model: Model) -> StaticCreature {
    StaticCreature::new(model, texture(), CreaturePlacement::demo()).unwrap()
}

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

/// Mundo con la criatura y la nave en el arranque `--near-creature`, sin modos.
fn creature_world(model: Model) -> World {
    let mut world = new_world();
    world.set_creature(creature(model)).unwrap();
    world.place_near_creature().unwrap();
    world
}

fn enable_stages(world: &mut World) {
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
}

fn gaze_world() -> World {
    let mut world = creature_world(offset_dome());
    enable_stages(&mut world);
    world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .unwrap();
    world
}

fn skin_of(model: &Model) -> EyeSkin {
    EyeSkin::calibrate(model, &dome_region()).unwrap()
}

/// Un triangulo degenerado: oculta la nave sin tocar su camara.
fn no_ship() -> Model {
    parse_obj("v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3\n").unwrap()
}

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Objetivos angulares hacia la nave, la camara y el punto al que mira la camara.
fn goals(world: &World) -> (GazeAngles, GazeAngles, GazeAngles) {
    let skin = skin_of(&offset_dome());
    let creature = world.creature().unwrap();
    (
        skin.aim(creature, world.ship.position),
        skin.aim(creature, world.camera.position),
        skin.aim(creature, world.camera.target),
    )
}

// --- Eleccion del objetivo ---

#[test]
fn choosing_the_target_changes_only_the_goal_and_the_gaze_eases_to_it() {
    // Sin mirada: rechazo limpio y sin efectos implicitos.
    let mut world = creature_world(offset_dome());
    for stages in [false, true] {
        if stages {
            enable_stages(&mut world);
        }
        let error = world
            .set_creature_eye_target(EyeTarget::Camera)
            .expect_err("sin mirada");
        assert!(error.contains("--creature-eye-follow"), "{error}");
        assert!(world.creature_eye_follow().is_none());
        assert!(world.creature_eye_target().is_none() && world.creature_eye_goal().is_none());
        assert_eq!(world.staged_creature_reveal().is_some(), stages);
    }
    world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .unwrap();
    assert_eq!(EyeTarget::default(), EyeTarget::Ship);
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Ship));
    let (ship, camera, look) = goals(&world);
    assert!(
        ship != camera && camera != look,
        "{ship:?} {camera:?} {look:?}"
    );
    assert_eq!(world.creature_eye_goal(), Some(ship));

    // Cambiar a la camara: ni salto de angulos ni recalibracion ni pose nueva.
    world.ship_model = no_ship();
    world.debug_oxygen01 = 0.05;
    world.snap_creature_reveal();
    let frame = render(&world);
    let state = |w: &World| {
        (
            w.creature_eye_follow(),
            w.creature_eye_skin().cloned(),
            w.creature_eye_cache_bytes(),
            w.creature_eye_pose_updates(),
            w.creature_eye_posed(),
        )
    };
    let before = state(&world);
    world.set_creature_eye_target(EyeTarget::Camera).unwrap();
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    assert_eq!(
        world.creature_eye_goal(),
        Some(camera),
        "camara, no su target"
    );
    assert_eq!(state(&world), before);
    assert_eq!(render(&world), frame);
    assert_eq!(state(&world), before);

    // La interpolacion existente (t95 1.5 s) llega al nuevo objetivo con tiempo real.
    let mut expected = EyeFollow::new(EyeFollowSettings::default());
    expected.snap(ship);
    expected.update(camera, 1.5);
    world.update_creature_eye_follow(1.5);
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        expected.current()
    );
    // Y de vuelta a la nave, igual de suave.
    world.set_creature_eye_target(EyeTarget::Ship).unwrap();
    assert_eq!(world.creature_eye_goal(), Some(ship));
    expected.update(ship, 0.4);
    world.update_creature_eye_follow(0.4);
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        expected.current()
    );
    assert_eq!(world.creature_eye_cache_bytes(), before.2);
}

#[test]
fn the_choice_survives_creature_changes_and_resets_snap_to_the_chosen_point() {
    let mut world = gaze_world();
    world.set_creature_eye_target(EyeTarget::Camera).unwrap();
    // Quitar y poner la criatura conserva la eleccion y salta a su objetivo.
    let c = world.clear_creature().unwrap();
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    assert!(
        world.creature_eye_goal().is_none(),
        "sin criatura no hay objetivo"
    );
    world.set_creature(c).unwrap();
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        goals(&world).1
    );
    // R y los arranques: conservan la eleccion y saltan con la camara ya recolocada.
    type Reset = fn(&mut World);
    let resets: [(&str, Reset); 3] = [
        ("reset_ship", |w| w.reset_ship()),
        ("place_near", |w| w.place_near(1, 25.0).unwrap()),
        ("place_near_creature", |w| w.place_near_creature().unwrap()),
    ];
    for (name, reset) in resets {
        world.ship.position += Vec3::new(-70.0, 30.0, 0.0);
        world.update(&Default::default(), 0.05);
        world.update_creature_eye_follow(0.2);
        reset(&mut world);
        let (ship, camera, _) = goals(&world);
        assert_eq!(
            world.creature_eye_target(),
            Some(EyeTarget::Camera),
            "{name}"
        );
        assert_eq!(
            world.creature_eye_follow().unwrap().current(),
            camera,
            "{name}"
        );
        assert_ne!(camera, ship, "{name}");
    }
    // Apagar y volver a activar: contrato normal, nave por defecto.
    world.disable_creature_eye_follow();
    assert!(world.set_creature_eye_target(EyeTarget::Camera).is_err());
    world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .unwrap();
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Ship));
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        goals(&world).0
    );
    // Arranque de la comparacion: ya mirando a la camara (salto determinista).
    world
        .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
        .unwrap();
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        goals(&world).1
    );
    // Cambiar de modo la apaga como siempre.
    enable_stages(&mut world);
    assert!(world.creature_eye_target().is_none());
}

// --- Diagnostico puro ---

use umbral::{
    math::Mat4,
    visual::creature_eye_follow::{EYE_FOLLOW_MAX_PITCH, EYE_FOLLOW_MAX_YAW, aim_gaze},
};

#[test]
fn the_off_axis_angle_is_the_one_the_fade_uses() {
    let pivot = Vec3::new(3.0, -2.0, 5.0);
    let axis = Vec3::new(0.05, 0.02, 1.0).normalized();
    for yaw in [0.0, 0.7, -2.0] {
        let axis_world = Mat4::rotation_y(yaw).transform_dir(axis);
        assert!(off_axis_degrees(pivot, axis, yaw, pivot + axis_world * 300.0).unwrap() < 1e-6);
        assert!(
            (off_axis_degrees(pivot, axis, yaw, pivot - axis_world * 2.0).unwrap() - 180.0).abs()
                < 1e-6
        );
        let side = axis_world.cross(Vec3::Y).normalized();
        let at = |deg: f64| {
            let r = deg.to_radians();
            pivot + (axis_world * r.cos() + side * r.sin()) * 50.0
        };
        for deg in [10.0, 74.0, 90.0, 120.0] {
            let got = off_axis_degrees(pivot, axis, yaw, at(deg)).unwrap();
            assert!((got - deg).abs() < 1e-9, "{yaw} {deg} {got}");
        }
    }
    // Degenerados: sin direccion.
    assert_eq!(off_axis_degrees(pivot, axis, 0.0, pivot), None);
    assert_eq!(
        off_axis_degrees(pivot, Vec3::ZERO, 0.0, pivot + Vec3::Z),
        None
    );
    assert_eq!(
        off_axis_degrees(pivot, axis, f64::NAN, pivot + Vec3::Z),
        None
    );
}

/// Estado del panel para un objetivo a `elevation` grados por encima (o debajo) del eje.
fn vertical_case(elevation: f64) -> (GazeAngles, GazeLimit) {
    let (pivot, axis) = (Vec3::ZERO, Vec3::Z);
    let r = elevation.to_radians();
    let target = Vec3::new(0.0, r.sin(), r.cos()) * 100.0;
    let goal = aim_gaze(pivot, axis, 0.0, target);
    let off = off_axis_degrees(pivot, axis, 0.0, target);
    (goal, gaze_limit(off, goal))
}

#[test]
fn up_and_down_the_current_limits_saturate_and_fade_whatever_the_target() {
    // Hallazgo de Veliona, sin cambiar la politica: pitch al tope de 8 grados de 10 a 75 y
    // desvanecido de 75 a 105, tambien arriba y abajo.
    for sign in [1.0, -1.0] {
        for elevation in [10.0, 30.0, 60.0, 75.0] {
            let (goal, limit) = vertical_case(sign * elevation);
            assert!(
                (goal.pitch() - sign * EYE_FOLLOW_MAX_PITCH).abs() < 1e-12,
                "{elevation}"
            );
            assert_eq!(limit, GazeLimit::Clamped, "{elevation}");
        }
        for elevation in [80.0, 90.0, 104.0] {
            let (goal, limit) = vertical_case(sign * elevation);
            assert!(goal.pitch().abs() < EYE_FOLLOW_MAX_PITCH && goal.pitch() * sign > 0.0);
            assert_eq!(limit, GazeLimit::Fading, "{elevation}");
        }
        for elevation in [105.0, 150.0] {
            let (goal, limit) = vertical_case(sign * elevation);
            assert!(goal.is_neutral());
            assert_eq!(limit, GazeLimit::Rest, "{elevation}");
        }
        let (goal, limit) = vertical_case(sign * 3.0);
        assert!((goal.pitch() - sign * 3.0_f64.to_radians()).abs() < 1e-12);
        assert_eq!(limit, GazeLimit::Inside);
    }
    assert_eq!(
        gaze_limit(None, GazeAngles::NEUTRAL),
        GazeLimit::NoDirection
    );
    // En la elipse (diagonal) tambien es tope.
    let diagonal = GazeAngles::clamped(EYE_FOLLOW_MAX_YAW, EYE_FOLLOW_MAX_PITCH);
    assert_eq!(gaze_limit(Some(20.0), diagonal), GazeLimit::Clamped);
}

#[test]
fn the_panel_text_is_ascii_for_the_bitmap_font_and_does_not_hide_the_state() {
    let g = |yaw: f64, pitch: f64| GazeAngles::clamped(yaw.to_radians(), pitch.to_radians());
    let status = EyeComparisonStatus {
        target: EyeTarget::Camera,
        current: g(3.24, -0.0),
        goal: Some(g(0.0, 8.0)),
        off_axis_degrees: Some(30.04),
    };
    assert_eq!(
        eye_comparison_lines(&status),
        [
            "MIRADA A: CAMARA   G CAMBIA A NAVE",
            "ACTUAL   YAW +3.2  PITCH +0.0",
            "OBJETIVO YAW +0.0  PITCH +8.0",
            "LIMITE 10/8: TOPE   EJE 30.0 GRADOS (FADE 75-105)",
        ]
    );
    let ship = EyeComparisonStatus {
        target: EyeTarget::Ship,
        current: GazeAngles::NEUTRAL,
        goal: Some(GazeAngles::NEUTRAL),
        off_axis_degrees: Some(110.0),
    };
    let lines = eye_comparison_lines(&ship);
    assert_eq!(lines[0], "MIRADA A: NAVE   G CAMBIA A CAMARA");
    assert_eq!(
        lines[3],
        "LIMITE 10/8: REPOSO   EJE 110.0 GRADOS (FADE 75-105)"
    );
    let fading = EyeComparisonStatus {
        off_axis_degrees: Some(90.0),
        goal: Some(g(0.0, 4.0)),
        ..ship
    };
    assert!(eye_comparison_lines(&fading)[3].starts_with("LIMITE 10/8: DESVANECE"));
    let inside = EyeComparisonStatus {
        off_axis_degrees: Some(2.0),
        goal: Some(g(1.0, 1.5)),
        ..ship
    };
    assert!(eye_comparison_lines(&inside)[3].starts_with("LIMITE 10/8: DENTRO"));
    let none = EyeComparisonStatus {
        goal: None,
        off_axis_degrees: None,
        ..ship
    };
    assert_eq!(
        eye_comparison_lines(&none)[1..],
        ["SIN CRIATURA: SIN OBJETIVO".to_string()]
    );
    // Solo caracteres que la fuente 5x7 del overlay dibuja.
    for s in [status, ship, fading, inside, none] {
        for line in eye_comparison_lines(&s) {
            assert!(
                line.chars().all(|c| c.is_ascii_uppercase()
                    || c.is_ascii_digit()
                    || " -.:/%()=+,|".contains(c)),
                "{line}"
            );
        }
    }
}

// --- Estado para el panel en el mundo ---

#[test]
fn the_world_reports_the_chosen_point_and_neither_target_lifts_the_limits() {
    let mut world = gaze_world();
    assert!(
        new_world().creature_eye_comparison_status().is_none(),
        "sin mirada"
    );
    let skin = skin_of(&offset_dome());
    let (pivot, axis, yaw) = {
        let c = world.creature().unwrap();
        let pivot = c.transform().matrix().transform_point(skin.pivot());
        (pivot, skin.optical_axis(), c.placement().yaw())
    };
    let expected = |world: &World, target: EyeTarget| {
        let point = match target {
            EyeTarget::Ship => world.ship.position,
            EyeTarget::Camera => world.camera.position,
        };
        EyeComparisonStatus {
            target,
            current: world.creature_eye_follow().unwrap().current(),
            goal: Some(skin.aim(world.creature().unwrap(), point)),
            off_axis_degrees: off_axis_degrees(pivot, axis, yaw, point),
        }
    };
    for target in [EyeTarget::Ship, EyeTarget::Camera, EyeTarget::Ship] {
        world.set_creature_eye_target(target).unwrap();
        assert_eq!(
            world.creature_eye_comparison_status(),
            Some(expected(&world, target))
        );
    }
    // Arriba y abajo, con la nave y la camara en sitios distintos: el mismo tope o desvanecido.
    let up = Vec3::Y.cross(axis).normalized().cross(axis).normalized() * -1.0;
    let at = |deg: f64, distance: f64| {
        let r = deg.to_radians();
        pivot + (axis * r.cos() + up * r.sin()) * distance
    };
    for sign in [1.0, -1.0] {
        for (deg, limit) in [
            (30.0, GazeLimit::Clamped),
            (60.0, GazeLimit::Clamped),
            (90.0, GazeLimit::Fading),
            (120.0, GazeLimit::Rest),
        ] {
            world.ship.position = at(sign * deg, 200.0);
            world.camera.position = at(sign * (deg + 3.0), 225.0);
            for target in [EyeTarget::Ship, EyeTarget::Camera] {
                world.set_creature_eye_target(target).unwrap();
                let status = world.creature_eye_comparison_status().unwrap();
                assert_eq!(status, expected(&world, target));
                let goal = status.goal.unwrap();
                assert_eq!(
                    gaze_limit(status.off_axis_degrees, goal),
                    limit,
                    "{sign} {deg} {target:?}"
                );
                if limit == GazeLimit::Clamped {
                    assert!(goal.pitch() * sign > 0.0);
                    let edge = (goal.yaw() / EYE_FOLLOW_MAX_YAW)
                        .hypot(goal.pitch() / EYE_FOLLOW_MAX_PITCH);
                    assert!((edge - 1.0).abs() < 1e-9, "{edge}");
                }
            }
        }
    }
    // Sin criatura: la eleccion sigue y no hay objetivo.
    world.clear_creature();
    let status = world.creature_eye_comparison_status().unwrap();
    assert_eq!((status.goal, status.off_axis_degrees), (None, None));
}

// --- Panel inferior ---

/// El texto mas largo posible (desvanecido con el angulo de 3 cifras).
fn longest_lines() -> Vec<String> {
    eye_comparison_lines(&EyeComparisonStatus {
        target: EyeTarget::Ship,
        current: GazeAngles::clamped(-0.17, -0.13),
        goal: Some(GazeAngles::clamped(-0.17, -0.13)),
        off_axis_degrees: Some(104.9),
    })
}

#[test]
fn the_panel_sits_at_the_bottom_fits_small_frames_and_only_touches_its_rectangle() {
    let lines = longest_lines();
    assert_eq!(lines.len(), 4);
    for (width, height, scale) in [(640, 360, 1), (320, 180, 1), (1280, 720, 2)] {
        let panel = eye_comparison_panel(width, height, &lines);
        assert_eq!(panel.scale, scale, "{width}x{height}");
        assert_eq!((panel.x, panel.y + panel.h), (0, height), "pegado abajo");
        assert!(panel.w <= width, "{width}: {}", panel.w);
        // Texto completo dentro del panel, sin subir hasta la banda del overlay ni al ojo.
        let text_w = lines.iter().map(|l| l.len()).max().unwrap() * 6 * scale;
        assert!(text_w + 8 <= panel.w || panel.w == width, "{width}");
        assert_eq!(panel.h, lines.len() * 10 * scale + 12);
        assert!(panel.y > height / 2, "{width}x{height}: {}", panel.y);

        let mut fb = Framebuffer::new(width, height, 0x808080).unwrap();
        draw_eye_comparison_panel(&mut fb, &lines);
        let inside = |i: usize| {
            let (x, y) = (i % width, i / width);
            x >= panel.x && x < panel.x + panel.w && y >= panel.y
        };
        let mut text = 0;
        for (i, &p) in fb.pixels.iter().enumerate() {
            if !inside(i) {
                assert_eq!(p, 0x808080, "fuera del panel {i}");
            } else if p != 0x808080 && p != 0x393939 {
                text += 1;
            }
        }
        assert!(
            text > 200 * scale * scale,
            "{width}: {text} pixeles de texto"
        );
    }
}

// --- Linea de comandos y App ---

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::{
    app::{GameOptions, run_game_with_eye_comparison},
    visual::staged_creature_reveal::CREATURE_EYE_REGION,
};

#[test]
fn compare_flags_need_the_eye_follow_and_reject_legacy() {
    for eye in [false, true] {
        for legacy in [false, true] {
            assert_eq!(validate_eye_compare_flags(false, eye, legacy), Ok(()));
        }
    }
    assert_eq!(validate_eye_compare_flags(true, true, false), Ok(()));
    for (eye, legacy, needle) in [
        (
            false,
            false,
            "--creature-eye-compare necesita --creature-eye-follow",
        ),
        (false, true, "--legacy no admite --creature-eye-compare"),
        (true, true, "--legacy no admite --creature-eye-compare"),
    ] {
        let error = validate_eye_compare_flags(true, eye, legacy).expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
}

#[test]
fn the_library_entry_point_validates_before_any_io() {
    let output =
        std::env::temp_dir().join(format!("umbral-eye-compare-lib-{}.png", std::process::id()));
    let options = |eye: bool, stages: bool| GameOptions {
        width: 64,
        height: 36,
        headless: true,
        frame_limit: None,
        output: output.clone(),
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: None,
        creature_reveal: false,
        creature_reveal_stages: stages,
        creature_eye_follow: eye,
        debug_oxygen: 1.0,
    };
    for (eye, stages, needle) in [
        (
            false,
            false,
            "--creature-eye-compare necesita --creature-eye-follow",
        ),
        (
            true,
            false,
            "--creature-eye-follow necesita --creature-reveal-stages",
        ),
        (
            true,
            true,
            "--creature-eye-follow necesita --creature-model",
        ),
    ] {
        let error = run_game_with_eye_comparison(options(eye, stages)).expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
    assert!(!output.exists());
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("umbral-eye-compare-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_texture(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160])
        .unwrap();
    writer.finish().unwrap();
}

/// `offset_dome` escalada y movida al centro de la ROI calibrada del asset real (la que usa
/// la linea de comandos).
fn calibrated_source() -> String {
    let (c, s) = (
        CREATURE_EYE_REGION.center(),
        CREATURE_EYE_REGION.radius() / 0.85,
    );
    let mut out = String::new();
    for line in offset_dome_source().lines() {
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

/// Siempre con `--headless`: estas pruebas nunca abren ventana.
fn run_game(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .output()
        .unwrap()
}

/// PNG RGB de 8 bits sin entrelazar a pixeles `0xRRGGBB`.
fn decode(bytes: &[u8]) -> (usize, usize, Vec<u32>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let pixels = data[..info.buffer_size()]
        .chunks_exact(3)
        .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
        .collect();
    (info.width as usize, info.height as usize, pixels)
}

#[test]
fn the_game_command_line_wires_the_comparison_and_validates_before_any_io() {
    let dir = temp_dir("cli");
    let (model, texture_path) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    write_texture(&texture_path);
    let texture_bytes = std::fs::read(&texture_path).unwrap();
    let output = dir.join("salida.png");
    let os = OsStr::new;
    let outcome = std::panic::catch_unwind(|| {
        let creature = [
            os("--creature-model"),
            model.as_os_str(),
            os("--creature-texture"),
            texture_path.as_os_str(),
        ];
        // Rutas inexistentes: cada error es de validacion, no de IO.
        let missing = [
            os("--creature-model"),
            os("no-existe.obj"),
            os("--creature-texture"),
            os("no-existe.png"),
        ];
        let compare = os("--creature-eye-compare");
        let eye = os("--creature-eye-follow");
        let stages = os("--creature-reveal-stages");
        let needs_eye = "--creature-eye-compare necesita --creature-eye-follow";
        let failing: Vec<(Vec<&OsStr>, &str)> = vec![
            (vec![compare], needs_eye),
            ([&missing[..], &[stages, compare]].concat(), needs_eye),
            (
                vec![os("--legacy"), compare],
                "--legacy no admite --creature-eye-compare",
            ),
            // Errores heredados de los flags previos.
            (
                [&missing[..], &[eye, compare]].concat(),
                "--creature-eye-follow necesita --creature-reveal-stages",
            ),
            (
                [
                    &missing[..],
                    &[os("--creature-reveal"), stages, eye, compare],
                ]
                .concat(),
                "incompatibles",
            ),
            (
                [
                    &missing[..],
                    &[stages, eye, compare, os("--width"), os("0")],
                ]
                .concat(),
                "--width debe ser mayor que cero",
            ),
        ];
        for (args, needle) in failing {
            let mut all = vec![os("--output"), output.as_os_str()];
            all.extend(args.iter().copied());
            let result = run_game(&all);
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert_eq!(result.status.code(), Some(1), "{args:?}: {stderr}");
            assert!(stderr.contains(needle), "{needle:?} no esta en {stderr}");
            assert!(!output.exists(), "{args:?} genero un PNG");
        }
        // La captura no puede sobrescribir una entrada.
        let mut args = vec![
            os("--output"),
            texture_path.as_os_str(),
            stages,
            eye,
            compare,
        ];
        args.extend(creature);
        let result = run_game(&args);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("coincide con la entrada"));
        assert_eq!(std::fs::read(&texture_path).unwrap(), texture_bytes);

        let capture = |name: &str, extra: &[&OsStr]| {
            let path = dir.join(name);
            let mut args = vec![os("--output"), path.as_os_str()];
            args.extend(extra.iter().copied());
            let result = run_game(&args);
            assert!(
                result.status.success(),
                "{extra:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            (
                std::fs::read(&path).unwrap(),
                String::from_utf8_lossy(&result.stdout).into_owned(),
            )
        };
        let size = [os("--width"), os("640"), os("--height"), os("360")];
        let near = [
            &creature[..],
            &[
                os("--near-creature"),
                stages,
                eye,
                os("--debug-oxygen"),
                os("0.05"),
            ],
            &size,
        ]
        .concat();
        let (follow_png, follow_out) = capture("mirada.png", &near);
        let (compare_png, compare_out) =
            capture("comparacion.png", &[&near[..], &[compare]].concat());
        // Arranca mirando a la camara (salto determinista) y lo dice.
        let mut world = World::new(load_model(&ship_model_path()).unwrap(), 640.0 / 360.0).unwrap();
        let bind = parse_obj(&calibrated_source()).unwrap();
        let skin = EyeSkin::calibrate(&bind, &CREATURE_EYE_REGION).unwrap();
        world
            .set_creature(StaticCreature::new(bind, texture(), CreaturePlacement::demo()).unwrap())
            .unwrap();
        world.place_near_creature().unwrap();
        let creature_ref = world.creature().unwrap();
        let (to_ship, to_camera) = (
            skin.aim(creature_ref, world.ship.position),
            skin.aim(creature_ref, world.camera.position),
        );
        let line = |g: GazeAngles, what: &str| {
            format!(
                "Mirada: yaw {:+.3} grados, pitch {:+.3} grados hacia la {what}",
                g.yaw().to_degrees(),
                g.pitch().to_degrees()
            )
        };
        assert!(follow_out.contains(&line(to_ship, "nave")), "{follow_out}");
        assert!(
            compare_out.contains(&line(to_camera, "camara")),
            "{compare_out}"
        );
        assert!(
            compare_out.contains("Comparacion de la mirada: G alterna NAVE/CAMARA"),
            "{compare_out}"
        );
        assert!(!follow_out.contains("Comparacion"), "{follow_out}");
        // Panel inferior en headless (sin --overlay); la banda superior no cambia.
        let (w, h, follow_px) = decode(&follow_png);
        let (_, _, compare_px) = decode(&compare_png);
        let status = EyeComparisonStatus {
            target: EyeTarget::Camera,
            current: to_camera,
            goal: Some(to_camera),
            off_axis_degrees: None,
        };
        let panel = eye_comparison_panel(w, h, &eye_comparison_lines(&status));
        assert!(panel.y > h / 2);
        assert!(
            (0..w * 40).all(|i| follow_px[i] == compare_px[i]),
            "banda superior intacta"
        );
        let darker = (panel.y * w..h * w)
            .filter(|&i| i % w < panel.w && compare_px[i] != follow_px[i])
            .count();
        assert!(darker > panel.w * panel.h / 4, "{darker}");
        // Mismo cuadro con --overlay: el panel persiste igual, abajo.
        let (with_overlay, _) = capture(
            "overlay.png",
            &[&near[..], &[compare, os("--overlay")]].concat(),
        );
        let (_, _, overlay_px) = decode(&with_overlay);
        assert!((panel.y * w..h * w).all(|i| overlay_px[i] == compare_px[i]));
        assert_ne!(overlay_px, compare_px);
    });
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}

// --- Trayectoria real: controles -> fisica -> camara de persecucion -> mirada -> render ---

use umbral::scene::ship::ShipInput;

/// Vuelo con empuje, cabeceo y giro: 90 cuadros de 1/60 s, al 5 % (criatura entera).
fn replay(world: &mut World, toggle_every: Option<usize>) -> Vec<(GazeAngles, GazeAngles)> {
    world.ship_model = no_ship();
    world.debug_oxygen01 = 0.05;
    world.snap_creature_reveal();
    let dt = 1.0 / 60.0;
    let start = world.creature_eye_follow().unwrap().current();
    let mut replica = EyeFollow::new(EyeFollowSettings::default());
    replica.snap(start);
    let bytes = world.creature_eye_cache_bytes();
    let mut trace = Vec::new();
    for frame in 0..90 {
        if toggle_every.is_some_and(|n| frame > 0 && frame % n == 0) {
            let next = match world.creature_eye_target().unwrap() {
                EyeTarget::Ship => EyeTarget::Camera,
                EyeTarget::Camera => EyeTarget::Ship,
            };
            world.set_creature_eye_target(next).unwrap();
        }
        let input = ShipInput {
            thrust: 1.0,
            pitch: if frame < 45 { 0.6 } else { -0.6 },
            yaw: 0.4,
            ..ShipInput::default()
        };
        world.update(&input, dt);
        world.update_creature_eye_follow(dt);
        // Identidad del objetivo con la camara real ya movida (nunca su target).
        let (ship, camera, look) = goals(world);
        let goal = world.creature_eye_goal().unwrap();
        match world.creature_eye_target().unwrap() {
            EyeTarget::Ship => assert_eq!(goal, ship, "cuadro {frame}"),
            EyeTarget::Camera => {
                assert_eq!(goal, camera, "cuadro {frame}");
                assert_ne!(goal, look, "cuadro {frame}");
            }
        }
        replica.update(goal, dt);
        let current = world.creature_eye_follow().unwrap().current();
        assert_eq!(current, replica.current(), "cuadro {frame}");
        let _ = render(world);
        assert_eq!(world.creature_eye_posed(), Some(current), "cuadro {frame}");
        assert_eq!(world.creature_eye_cache_bytes(), bytes);
        trace.push((current, goal));
    }
    assert!(world.creature_eye_pose_updates().unwrap() <= 90);
    trace
}

#[test]
fn a_flown_trajectory_follows_the_chosen_point_and_the_ship_default_is_unchanged() {
    // Dos formas del default (nave) dan exactamente el mismo vuelo y los mismos cuadros.
    let mut a = gaze_world();
    let mut b = creature_world(offset_dome());
    enable_stages(&mut b);
    b.enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Ship)
        .unwrap();
    let ship_trace = replay(&mut a, None);
    assert_eq!(replay(&mut b, None), ship_trace);
    assert_eq!(render(&a), render(&b));

    let mut camera = gaze_world();
    camera.set_creature_eye_target(EyeTarget::Camera).unwrap();
    let camera_trace = replay(&mut camera, None);
    let mut toggled = gaze_world();
    let toggled_trace = replay(&mut toggled, Some(20));
    // Diferencia real entre apuntar a la nave y a la camara en esta trayectoria (grados).
    let max_diff = ship_trace
        .iter()
        .zip(&camera_trace)
        .map(|((s, _), (c, _))| {
            (s.yaw() - c.yaw())
                .to_degrees()
                .abs()
                .max((s.pitch() - c.pitch()).to_degrees().abs())
        })
        .fold(0.0, f64::max);
    let clamped = |t: &[(GazeAngles, GazeAngles)]| {
        t.iter()
            .filter(|(_, g)| {
                (g.yaw() / umbral::visual::creature_eye_follow::EYE_FOLLOW_MAX_YAW)
                    .hypot(g.pitch() / EYE_FOLLOW_MAX_PITCH)
                    >= 1.0 - 1e-9
            })
            .count()
    };
    println!(
        "Trayectoria de 90 cuadros: diferencia maxima nave/camara {max_diff:.3} grados; cuadros con objetivo en el tope: nave {}, camara {}, alternando {}",
        clamped(&ship_trace),
        clamped(&camera_trace),
        clamped(&toggled_trace)
    );
    assert!(
        max_diff > 0.0,
        "la camara y la nave estan en sitios distintos"
    );
    assert_ne!(toggled_trace, ship_trace);
}

// Supervisor-prepared regression: direct public entry, synthetic disposable inputs only.
#[test]
fn library_review_rejects_texture_output_alias_without_modifying_inputs() {
    let dir = temp_dir("library-alias-review");
    let model = dir.join("fixture.obj");
    let texture_path = dir.join("fixture.png");
    std::fs::write(&model, calibrated_source()).unwrap();
    write_texture(&texture_path);
    let original_model = std::fs::read(&model).unwrap();
    let original_texture = std::fs::read(&texture_path).unwrap();
    let requested = umbral::visual::static_creature::CreatureOptions::from_flags(
        Some(model.clone()),
        Some(texture_path.clone()),
        true,
        None,
        false,
    )
    .unwrap();
    let result = run_game_with_eye_comparison(GameOptions {
        width: 64,
        height: 36,
        headless: true,
        frame_limit: None,
        output: texture_path.clone(),
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: requested,
        creature_reveal: false,
        creature_reveal_stages: true,
        creature_eye_follow: true,
        debug_oxygen: 0.05,
    });
    let after_model = std::fs::read(&model).unwrap();
    let after_texture = std::fs::read(&texture_path).unwrap();
    println!(
        "DIRECT_LIBRARY_ALIAS result={result:?} model_preserved={} texture_preserved={}",
        original_model == after_model,
        original_texture == after_texture
    );
    std::fs::remove_dir_all(&dir).unwrap();
    let error = result.expect_err("library must reject alias before any IO");
    assert!(error.contains("coincide con la entrada"), "{error}");
    assert_eq!(after_model, original_model);
    assert_eq!(after_texture, original_texture);
}

#[test]
fn library_review_rejects_incompatible_reveal_modes_before_missing_input_io() {
    let dir = temp_dir("library-reveal-modes-review");
    let model = dir.join("missing.obj");
    let texture_path = dir.join("missing.png");
    let output = dir.join("output.png");
    assert!(!model.exists() && !texture_path.exists() && !output.exists());
    let requested = umbral::visual::static_creature::CreatureOptions::from_flags(
        Some(model.clone()),
        Some(texture_path.clone()),
        false,
        None,
        false,
    )
    .unwrap();
    let result = run_game_with_eye_comparison(GameOptions {
        width: 64,
        height: 36,
        headless: true,
        frame_limit: None,
        output: output.clone(),
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: requested,
        creature_reveal: true,
        creature_reveal_stages: true,
        creature_eye_follow: true,
        debug_oxygen: 0.05,
    });
    println!("DIRECT_LIBRARY_REVEAL_MODES result={result:?}");
    let no_files_created = !model.exists() && !texture_path.exists() && !output.exists();
    std::fs::remove_dir_all(&dir).unwrap();
    let error = result.expect_err("incompatible reveal modes must fail before input IO");
    assert!(error.contains("incompatibles"), "{error}");
    assert!(
        error.contains("--creature-reveal y --creature-reveal-stages"),
        "{error}"
    );
    assert!(no_files_created);
}
