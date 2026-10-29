//! Sol con ojo en el juego real (`--creature-solar-reveal`): flags, vistas, arranque frente a la
//! receta del preview 03, teclas, bucle real (`ShipInput -> World -> seguimiento -> render`),
//! entrada de biblioteca y binario. Fixtures sinteticos desechables salvo las dos pruebas
//! `#[ignore]` del asset real (rutas por variables de entorno, nunca en el codigo).

use umbral::visual::solar_game::{
    SOLAR_CREATURE_RADIUS, SOLAR_CREATURE_YAW, SolarRevealRequest, SolarView,
    validate_solar_reveal_flags,
};

// --- G1: combinaciones de la CLI (puras, sin archivos) ---

fn request() -> SolarRevealRequest {
    SolarRevealRequest {
        has_creature: true,
        ..SolarRevealRequest::default()
    }
}

#[test]
fn the_solar_flag_needs_the_creature_and_rejects_every_conflicting_route() {
    let everything = SolarRevealRequest {
        has_creature: false,
        legacy: true,
        near_body: true,
        near_creature: true,
        uniform_reveal: true,
        distant_turn: true,
    };
    assert_eq!(validate_solar_reveal_flags(false, everything), Ok(()));
    assert_eq!(validate_solar_reveal_flags(true, request()), Ok(()));
    let cases = [
        (
            SolarRevealRequest {
                legacy: true,
                has_creature: false,
                ..request()
            },
            "--legacy no admite --creature-solar-reveal",
        ),
        (
            SolarRevealRequest {
                has_creature: false,
                ..request()
            },
            "--creature-solar-reveal necesita --creature-model y --creature-texture",
        ),
        (
            SolarRevealRequest {
                uniform_reveal: true,
                ..request()
            },
            "--creature-solar-reveal y --creature-reveal son incompatibles",
        ),
        (
            SolarRevealRequest {
                near_body: true,
                ..request()
            },
            "--creature-solar-reveal y --near son incompatibles",
        ),
        (
            SolarRevealRequest {
                near_creature: true,
                ..request()
            },
            "--creature-solar-reveal y --near-creature son incompatibles",
        ),
        (
            SolarRevealRequest {
                distant_turn: true,
                ..request()
            },
            "--creature-solar-reveal y --creature-distant-turn son incompatibles",
        ),
    ];
    for (case, needle) in cases {
        let error = validate_solar_reveal_flags(true, case).expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
}

#[test]
fn the_views_and_the_creature_are_the_approved_preview_values() {
    assert_eq!(SolarView::Frontal.profile(), (0.0, 298.0, 0.0));
    // La misma expresion que `examples/solar_reveal_preview.rs` (70 grados en radianes).
    let deg = std::f64::consts::PI / 180.0;
    assert_eq!(SolarView::Oblique.profile(), (70.0 * deg, 578.0, 60.0));
    assert_eq!((SOLAR_CREATURE_RADIUS, SOLAR_CREATURE_YAW), (80.0, 0.0));
}

// --- Fixtures sinteticos (cupula calibrada a la ROI del asset real, la de produccion) ---

use umbral::{
    app::advance_frame,
    assets::{
        obj::{Model, load_model, parse_obj},
        ship_model_path,
    },
    math::Vec3,
    renderer::{framebuffer::Framebuffer, stats::RenderStats, texture::Texture},
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_eye_comparison::EyeTarget,
        creature_eye_follow::EyeFollowSettings,
        creature_horizontal_follow::HORIZONTAL_FOLLOW_MAX_SPEED,
        creature_reveal::step_debug_oxygen,
        solar_game::{SolarGame, SolarKeys, solar_creature_placement},
        solar_reveal_preview::SolarRevealLight,
        staged_creature_reveal::{CREATURE_EYE_REGION, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const W: usize = 320;
const H: usize = 180;
const DT: f64 = 1.0 / 60.0;
/// Niveles de O2 de la matriz aprobada del preview 03.
const LEVELS: [f64; 8] = [1.0, 0.50, 0.40, 0.30, 0.28, 0.25, 0.15, 0.05];

/// Cupula de radio 1 con apice +Z, anillos de 10 grados y 16 segmentos.
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

/// Cupula con un triangulo de cuerpo desplazado, escalada y movida al centro de la ROI calibrada
/// del asset real (la que usan el juego y la linea de comandos).
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

fn creature_model() -> Model {
    parse_obj(&calibrated_source()).unwrap()
}

fn creature(placement: CreaturePlacement) -> StaticCreature {
    let texture = Texture::from_rgb8(2, 2, TEXELS.to_vec()).unwrap();
    StaticCreature::new(creature_model(), texture, placement).unwrap()
}

fn new_world(width: usize, height: usize) -> World {
    World::new(
        load_model(&ship_model_path()).unwrap(),
        width as f64 / height as f64,
    )
    .unwrap()
}

fn with_oxygen(mut world: World, oxygen: f64) -> World {
    world.debug_oxygen01 = oxygen;
    world.snap_creature_reveal();
    world
}

/// La receta del preview 03 aprobado (`examples/solar_reveal_preview.rs`: `build` y `place`)
/// armada a mano con la API publica; `settle` repite sus 20 s de mirada y seguimiento.
fn preview_recipe(width: usize, height: usize, oxygen: f64, settle: bool) -> World {
    let mut world = new_world(width, height);
    world.toggles.freeze_orbits = true;
    let sun = world
        .system
        .bodies
        .iter()
        .find(|b| b.orbit.radius == 0.0)
        .unwrap()
        .position(world.time);
    world
        .set_creature(creature(CreaturePlacement::new(sun, 80.0, 0.0).unwrap()))
        .unwrap();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), CREATURE_EYE_REGION)
        .unwrap();
    world
        .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
        .unwrap();
    world.enable_creature_horizontal_follow().unwrap();
    world
        .enable_solar_reveal_preview(SolarRevealLight::Camera)
        .unwrap();
    world.place_solar_reveal_camera(0.0, 298.0, 0.0).unwrap();
    if settle {
        for _ in 0..(20.0 / DT).round() as usize {
            world.update_creature_eye_follow(DT);
        }
    }
    with_oxygen(world, oxygen)
}

/// El arranque del juego: el O2 de depuracion ya puesto (como `--debug-oxygen`) y `SolarGame`.
fn game_world(width: usize, height: usize, oxygen: f64) -> (World, SolarGame) {
    let mut world = new_world(width, height);
    world.debug_oxygen01 = oxygen;
    let placement = solar_creature_placement(&world).unwrap();
    let game = SolarGame::start(&mut world, creature(placement)).unwrap();
    (world, game)
}

fn solar_frame(world: &World) -> Framebuffer {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    world.render_solar_reveal_preview(&mut fb).unwrap();
    fb
}

fn game_frame(game: &SolarGame, world: &World) -> (Framebuffer, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = game.render(world, &mut fb).unwrap();
    (fb, stats)
}

fn base_frame(world: &World) -> Framebuffer {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    world.render(&mut fb);
    fb
}

fn sun_center(world: &World) -> Vec3 {
    world.system.bodies[0].position(world.time)
}

// --- G2: arranque = receta del preview 03 (frontal, O2 100/50/40/30/28/25/15/5) ---

#[test]
fn the_start_is_the_approved_preview_recipe_in_state_and_pixels_at_every_level() {
    for oxygen in LEVELS {
        let (world, game) = game_world(W, H, oxygen);
        let recipe = preview_recipe(W, H, oxygen, false);
        assert_eq!(game.view(), SolarView::Frontal);
        let placement = world.creature().unwrap().placement();
        assert_eq!(placement, recipe.creature().unwrap().placement());
        assert_eq!(placement.center(), sun_center(&world));
        assert_eq!((placement.world_radius(), placement.yaw()), (80.0, 0.0));
        assert_eq!(world.ship.position, recipe.ship.position);
        assert_eq!(world.ship.orientation.m, recipe.ship.orientation.m);
        assert_eq!(
            (world.camera.position, world.camera.target, world.camera.up),
            (
                recipe.camera.position,
                recipe.camera.target,
                recipe.camera.up
            )
        );
        assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
        let follow = world.creature_horizontal_follow().unwrap();
        assert!(follow.is_active() && follow.yaw() == 0.0);
        assert_eq!(world.solar_reveal_alphas(), recipe.solar_reveal_alphas());
        assert_eq!(world.debug_oxygen01, oxygen);
        assert_eq!(world.time, 0.0);
        assert!(
            !world.toggles.freeze_orbits,
            "el juego conserva las orbitas vivas"
        );
        let (frame, _) = game_frame(&game, &world);
        assert!(
            frame.pixels == solar_frame(&recipe).pixels,
            "O2 {oxygen}: el arranque no es el cuadro de la receta"
        );
    }
}

#[test]
fn in_the_frontal_view_the_start_already_is_the_settled_preview_frame() {
    // Arranque (sin los 20 s) frente a asentado (con los 20 s del preview): misma imagen.
    for oxygen in [0.40, 0.28, 0.05] {
        let (world, game) = game_world(W, H, oxygen);
        let settled = preview_recipe(W, H, oxygen, true);
        let (frame, _) = game_frame(&game, &world);
        assert!(frame.pixels == solar_frame(&settled).pixels, "O2 {oxygen}");
        assert_eq!(
            world.creature_eye_follow().unwrap().current(),
            settled.creature_eye_follow().unwrap().current()
        );
        assert_eq!(
            world.creature_horizontal_follow().unwrap().yaw(),
            settled.creature_horizontal_follow().unwrap().yaw()
        );
    }
}

#[test]
fn the_creature_is_placed_in_the_sun_center_whatever_the_orbital_time() {
    for time in [0.0, 12.5, 4321.0] {
        let mut world = new_world(W, H);
        world.time = time;
        let placement = solar_creature_placement(&world).unwrap();
        assert_eq!(placement.center(), sun_center(&world));
        assert_eq!((placement.world_radius(), placement.yaw()), (80.0, 0.0));
    }
}

// --- G3: el cuadro del juego solar es siempre el del preview (inicial y cada cuadro) ---

#[test]
fn the_game_frame_is_the_solar_frame_and_only_equals_the_base_frame_with_the_eye_hidden() {
    for oxygen in LEVELS {
        let (world, game) = game_world(W, H, oxygen);
        let (frame, _) = game_frame(&game, &world);
        assert!(frame.pixels == solar_frame(&world).pixels, "O2 {oxygen}");
        let hidden = world.solar_reveal_alphas().unwrap().eye <= 0.0;
        assert_eq!(
            frame.pixels == base_frame(&world).pixels,
            hidden,
            "O2 {oxygen}: ojo oculto {hidden}"
        );
        assert_eq!(
            world.solar_reveal_buffer_bytes() > 0,
            !hidden,
            "O2 {oxygen}"
        );
    }
}

// --- G4: teclas 7/8/R/H/G (mismo helper que la ventana) ---

fn press(keys: SolarKeys, game: &mut SolarGame, world: &mut World) {
    game.apply(world, keys).unwrap();
}

const OBLIQUE: SolarKeys = SolarKeys {
    oblique: true,
    reset: false,
    frontal: false,
    follow: false,
    target: false,
};

/// Pose de referencia: la API aprobada del preview sobre un mundo de referencia.
fn expected_pose(view: SolarView) -> (Vec3, Vec3) {
    let mut reference = preview_recipe(W, H, 1.0, false);
    let (azimuth, distance, height) = view.profile();
    reference
        .place_solar_reveal_camera(azimuth, distance, height)
        .unwrap();
    (reference.ship.position, reference.camera.position)
}

#[test]
fn views_reset_follow_and_target_keep_oxygen_time_and_the_body_turns_without_a_new_snap() {
    let (mut world, mut game) = game_world(W, H, 0.28);
    world.time = 12.5;
    let shown = world.staged_creature_reveal().unwrap().shown();
    // 8: oblicua aceptada; el cuerpo NO salta: sigue en su giro y avanza con segundos reales.
    press(OBLIQUE, &mut game, &mut world);
    assert_eq!(game.view(), SolarView::Oblique);
    assert_eq!(
        (world.ship.position, world.camera.position),
        expected_pose(SolarView::Oblique)
    );
    assert_eq!(world.creature_horizontal_follow().unwrap().yaw(), 0.0);
    assert_eq!((world.debug_oxygen01, world.time), (0.28, 12.5));
    assert_eq!(world.staged_creature_reveal().unwrap().shown(), shown);
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    for _ in 0..60 {
        advance_frame(&mut world, &ShipInput::default(), DT);
    }
    let yaw = world.creature_horizontal_follow().unwrap().yaw();
    assert!(
        yaw > 0.0 && yaw <= HORIZONTAL_FOLLOW_MAX_SPEED * 1.0 + 1e-12,
        "un segundo real hacia la camara oblicua, sin salto: {yaw}"
    );
    // R vuelve al frontal (arranque); el O2, el tiempo y las elecciones se conservan.
    let time = world.time;
    let input = ShipInput {
        thrust: 1.0,
        yaw: 1.0,
        ..ShipInput::default()
    };
    for _ in 0..30 {
        advance_frame(&mut world, &input, DT);
    }
    assert_ne!(world.ship.position, expected_pose(SolarView::Oblique).0);
    let turned = world.creature_horizontal_follow().unwrap().yaw();
    let keys = |f: fn(&mut SolarKeys)| {
        let mut k = SolarKeys::default();
        f(&mut k);
        k
    };
    press(keys(|k| k.reset = true), &mut game, &mut world);
    assert_eq!(game.view(), SolarView::Frontal);
    assert_eq!(
        (world.ship.position, world.camera.position),
        expected_pose(SolarView::Frontal)
    );
    assert_eq!(world.debug_oxygen01, 0.28);
    assert!(world.time > time, "R no reinicia el reloj orbital");
    // R no salta el cuerpo: conserva su giro y sigue con su dinamica.
    assert_eq!(world.creature_horizontal_follow().unwrap().yaw(), turned);
    // 8 y 7 colocan las vistas.
    press(OBLIQUE, &mut game, &mut world);
    press(keys(|k| k.frontal = true), &mut game, &mut world);
    assert_eq!(game.view(), SolarView::Frontal);
    assert_eq!(
        (world.ship.position, world.camera.position),
        expected_pose(SolarView::Frontal)
    );
    // H apaga/enciende el seguimiento; G alterna CAMARA/NAVE. Nada mas cambia.
    press(keys(|k| k.follow = true), &mut game, &mut world);
    assert!(!world.creature_horizontal_follow().unwrap().is_active());
    press(keys(|k| k.target = true), &mut game, &mut world);
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Ship));
    assert_eq!(game.view(), SolarView::Frontal);
    press(keys(|k| k.follow = true), &mut game, &mut world);
    press(keys(|k| k.target = true), &mut game, &mut world);
    assert!(world.creature_horizontal_follow().unwrap().is_active());
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    assert_eq!(world.debug_oxygen01, 0.28);
    // Sin teclas no cambia nada.
    let before = (world.ship.position, world.camera.position, game.view());
    press(SolarKeys::default(), &mut game, &mut world);
    assert_eq!(
        (world.ship.position, world.camera.position, game.view()),
        before
    );
}

#[test]
fn the_window_title_reports_oxygen_target_follow_and_view() {
    let (mut world, mut game) = game_world(W, H, 0.28);
    let title = game.title(&world);
    for part in ["O2 28%", "G CAMARA", "H ON", "FRONTAL"] {
        assert!(title.contains(part), "{part:?} no esta en {title:?}");
    }
    press(OBLIQUE, &mut game, &mut world);
    let k = SolarKeys {
        follow: true,
        target: true,
        ..SolarKeys::default()
    };
    press(k, &mut game, &mut world);
    world.debug_oxygen01 = 0.05;
    let title = game.title(&world);
    for part in ["O2 5%", "G NAVE", "H OFF", "OBLICUA"] {
        assert!(title.contains(part), "{part:?} no esta en {title:?}");
    }
}

// --- G5: bucle real (ShipInput -> World::update -> camara -> seguimiento -> ojo -> render) ---

#[test]
fn flying_with_real_input_tracks_reveals_both_ways_and_every_frame_is_the_solar_frame() {
    let (mut world, game) = game_world(W, H, 0.35);
    let placement = world.creature().unwrap().placement();
    let skin = world.creature_eye_skin().unwrap() as *const _;
    let parts = world.staged_creature_parts().unwrap() as *const _;
    let start_azimuth = world
        .creature_horizontal_follow_status()
        .unwrap()
        .target_azimuth
        .unwrap();
    let mut previous_yaw = 0.0;
    let mut sun_alphas = Vec::new();
    let mut frames = 0;
    // 6 s de vuelo real: ~90 grados de giro (70 cuadros a 1.4 rad/s) y despues avance a fondo
    // hacia -X, asi la camara rodea el Sol (a giro y empuje constantes la nave solo describe un
    // circulo de ~17 u). El O2 baja de 5 en 5 con N (35 -> 10) y vuelve a subir con M
    // (10 -> 40), cruzando 30/25 en los dos sentidos.
    for frame in 0..360 {
        let (more, less) = (
            frame >= 180 && frame % 30 == 0,
            frame < 180 && frame % 30 == 0 && frame > 0,
        );
        world.debug_oxygen01 = step_debug_oxygen(world.debug_oxygen01, more, less);
        let (thrust, yaw) = if frame < 70 { (0.0, 1.0) } else { (1.0, 0.0) };
        let input = ShipInput {
            thrust,
            yaw,
            ..ShipInput::default()
        };
        // El cuadro inicial (frame 0) se dibuja antes de avanzar, como en la ventana.
        if frame > 0 {
            advance_frame(&mut world, &input, DT);
        }
        if frame < 3 || frame % 20 == 0 {
            let (fb, _) = game_frame(&game, &world);
            assert!(
                fb.pixels == solar_frame(&world).pixels,
                "cuadro {frame}: no es el cuadro solar"
            );
            // Inicial y segundo/tercer cuadro, con la pareja en vista y el ojo visible: no es
            // `World::render`. Despues la nave gira y la pareja puede salir del cuadro, donde
            // los dos cuadros coinciden legitimamente.
            if frame < 3 {
                assert!(world.solar_reveal_alphas().unwrap().eye > 0.0);
                assert!(
                    fb.pixels != base_frame(&world).pixels,
                    "cuadro {frame}: es World::render"
                );
            }
            frames += 1;
        }
        let follow = world.creature_horizontal_follow().unwrap();
        let step = (follow.yaw() - previous_yaw).abs();
        assert!(step <= HORIZONTAL_FOLLOW_MAX_SPEED * DT * (1.0 + 1e-9) + 1e-15);
        previous_yaw = follow.yaw();
        sun_alphas.push(world.solar_reveal_alphas().unwrap().sun);
        assert_eq!(world.creature().unwrap().placement(), placement);
    }
    assert!(frames >= 20);
    let status = world.creature_horizontal_follow_status().unwrap();
    let moved = (status.target_azimuth.unwrap() - start_azimuth).abs();
    assert!(moved > 2f64.to_radians(), "la camara no rodeo: {moved}");
    assert!(
        status.follow.yaw().abs() > 0.5f64.to_radians(),
        "el cuerpo no siguio"
    );
    // El Sol se funde al bajar (30 -> 25) y vuelve al subir, sin saltos de un cuadro.
    let (down, up) = sun_alphas.split_at(180);
    assert!(down.windows(2).all(|w| w[1] <= w[0]), "{down:?}");
    assert!(up.windows(2).all(|w| w[1] >= w[0]), "{up:?}");
    assert!(down.iter().any(|&a| a < 1.0) && *down.last().unwrap() < 0.5);
    assert!(sun_alphas.windows(2).all(|w| (w[1] - w[0]).abs() < 0.2));
    // Una carga, una calibracion y una particion: las mismas en todo el vuelo.
    assert!(std::ptr::eq(world.creature_eye_skin().unwrap(), skin));
    assert!(std::ptr::eq(world.staged_creature_parts().unwrap(), parts));
}

#[test]
fn live_orbits_and_the_sun_spin_never_drag_the_creature_and_f5_only_freezes_orbits() {
    let (mut world, mut game) = game_world(W, H, 0.28);
    press(OBLIQUE, &mut game, &mut world);
    for _ in 0..120 {
        advance_frame(&mut world, &ShipInput::default(), DT);
    }
    assert!(world.time > 1.9, "las orbitas siguen vivas: {}", world.time);
    let spin = world.system.bodies[0].rotation_angle(world.time);
    assert!(spin != world.system.bodies[0].rotation_angle(0.0));
    let creature = world.creature().unwrap();
    let p = creature.placement();
    let yaw = world.creature_horizontal_follow().unwrap().yaw();
    let expected = CreaturePlacement::new(p.center(), p.world_radius(), p.yaw() + yaw)
        .unwrap()
        .transform_for(
            Vec3::from_array(creature.model().bounds_center),
            creature.model().bounds_radius,
        )
        .unwrap();
    assert_eq!(world.creature_render_transform(), Some(expected));
    assert_eq!(p.center(), sun_center(&world));
    // F5: el reloj orbital se para; la revelacion y el seguimiento siguen con segundos reales.
    world.toggles.freeze_orbits = true;
    let time = world.time;
    world.debug_oxygen01 = 0.40;
    let (shown, yaw) = (world.staged_creature_reveal().unwrap().shown(), yaw);
    for _ in 0..30 {
        advance_frame(&mut world, &ShipInput::default(), DT);
    }
    assert_eq!(world.time, time);
    assert!(world.staged_creature_reveal().unwrap().shown() != shown);
    assert!(world.creature_horizontal_follow().unwrap().yaw() != yaw);
    let (fb, _) = game_frame(&game, &world);
    assert!(fb.pixels == solar_frame(&world).pixels);
}

#[test]
fn advance_frame_is_the_game_order_with_the_physics_dt_capped_and_real_seconds_for_effects() {
    // Mundo de referencia con las llamadas sueltas del bucle: update(dt <= 0.1), revelacion y
    // mirada con los segundos reales completos.
    let (mut world, _game) = game_world(W, H, 0.28);
    let (mut reference, _) = game_world(W, H, 0.28);
    let input = ShipInput {
        thrust: 1.0,
        yaw: 0.5,
        ..ShipInput::default()
    };
    for elapsed in [DT, 0.5, DT, 0.25] {
        world.debug_oxygen01 = 0.40;
        reference.debug_oxygen01 = 0.40;
        advance_frame(&mut world, &input, elapsed);
        reference.update(&input, elapsed.min(0.1));
        reference.update_creature_reveal(elapsed);
        reference.update_creature_eye_follow(elapsed);
        assert_eq!(world.time, reference.time);
        assert_eq!(world.ship.position, reference.ship.position);
        assert_eq!(world.camera.position, reference.camera.position);
        assert_eq!(
            world.staged_creature_reveal().unwrap().shown(),
            reference.staged_creature_reveal().unwrap().shown()
        );
        assert_eq!(
            world.creature_horizontal_follow().unwrap().yaw(),
            reference.creature_horizontal_follow().unwrap().yaw()
        );
        assert_eq!(
            world.creature_eye_follow().unwrap().current(),
            reference.creature_eye_follow().unwrap().current()
        );
    }
}

// Caracterizacion (despues del GREEN): arranque frente a asentado en la vista OBLICUA. El juego
// no simula los 20 s del preview: al pulsar 8 el cuerpo sigue de frente y llega con su dinamica.
#[test]
fn the_oblique_view_arrives_with_real_seconds_at_the_settled_preview_frame() {
    let (mut world, mut game) = game_world(W, H, 0.28);
    // F5 (orbitas congeladas en t = 0, como el preview) para comparar la misma escena.
    world.toggles.freeze_orbits = true;
    press(OBLIQUE, &mut game, &mut world);
    let mut settled = preview_recipe(W, H, 0.28, true);
    settled
        .place_solar_reveal_camera(70.0 * DEG, 578.0, 60.0)
        .unwrap();
    for _ in 0..(20.0 / DT).round() as usize {
        settled.update_creature_eye_follow(DT);
    }
    let (arrival, _) = game_frame(&game, &world);
    assert_eq!(world.creature_horizontal_follow().unwrap().yaw(), 0.0);
    assert!(
        arrival.pixels != solar_frame(&settled).pixels,
        "al pulsar 8 aun no esta asentado"
    );
    for _ in 0..(20.0 / DT).round() as usize {
        advance_frame(&mut world, &ShipInput::default(), DT);
    }
    assert_eq!(world.time, 0.0);
    let (yaw, goal) = (
        world.creature_horizontal_follow().unwrap().yaw(),
        settled.creature_horizontal_follow().unwrap().yaw(),
    );
    let (gaze, gaze_goal) = (
        world.creature_eye_follow().unwrap().current(),
        settled.creature_eye_follow().unwrap().current(),
    );
    println!("OBLICUA asentada: cuerpo {yaw:e} vs {goal:e}; ojo {gaze:?} vs {gaze_goal:?}");
    assert_eq!((yaw, gaze), (goal, gaze_goal));
    let (frame, _) = game_frame(&game, &world);
    assert!(frame.pixels == solar_frame(&settled).pixels);
}

const DEG: f64 = std::f64::consts::PI / 180.0;

// --- G6: binario y entrada de biblioteca (validan antes de cualquier IO) ---

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::{
    app::{GameOptions, run_game_with_solar_reveal},
    visual::static_creature::CreatureOptions,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-solar-game-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Modelo y textura desechables (la misma cupula calibrada) en `dir`.
fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let (model, texture) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    let file = std::fs::File::create(&texture).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&TEXELS).unwrap();
    writer.finish().unwrap();
    (model, texture)
}

/// Siempre con `--headless`: estas pruebas nunca abren ventana.
fn run_game(args: &[OsString]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .output()
        .unwrap()
}

fn args(parts: &[&OsStr]) -> Vec<OsString> {
    parts.iter().map(|p| p.to_os_string()).collect()
}

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

fn creature_args(model: &Path, texture: &Path, output: &Path, extra: &[&str]) -> Vec<OsString> {
    let o = OsStr::new;
    let mut a = args(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture.as_os_str(),
        o("--output"),
        output.as_os_str(),
        o("--width"),
        o("320"),
        o("--height"),
        o("180"),
    ]);
    a.extend(extra.iter().map(OsString::from));
    a
}

#[test]
fn the_command_line_rejects_conflicts_and_bad_outputs_before_any_io() {
    let dir = temp_dir("cli-guards");
    let (no_model, no_texture) = (dir.join("no.obj"), dir.join("no.png"));
    let output = dir.join("salida.png");
    let solar = "--creature-solar-reveal";
    let cases: [(&[&str], &str); 6] = [
        (
            &[solar, "--creature-reveal"],
            "--creature-reveal son incompatibles",
        ),
        (
            &[solar, "--near", "0"],
            "--creature-solar-reveal y --near son",
        ),
        (
            &[solar, "--near-creature"],
            "--near-creature son incompatibles",
        ),
        (
            &[solar, "--creature-eye-compare", "--creature-distant-turn"],
            "--creature-distant-turn son incompatibles",
        ),
        (&[solar, "--legacy"], "--legacy no admite"),
        (&[solar, "--debug-oxygen", "2"], "--debug-oxygen necesita"),
    ];
    for (extra, needle) in cases {
        let out = run_game(&creature_args(&no_model, &no_texture, &output, extra));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{needle}: {stderr}");
        assert!(stderr.contains(needle), "{needle:?} no esta en {stderr:?}");
        assert!(!output.exists() && !no_model.exists() && !no_texture.exists());
    }
    // Sin criatura (y con --legacy sin criatura): el mensaje del flag solar.
    for (extra, needle) in [
        (
            vec![solar],
            "--creature-solar-reveal necesita --creature-model y --creature-texture",
        ),
        (
            vec![solar, "--legacy"],
            "--legacy no admite --creature-solar-reveal",
        ),
    ] {
        let mut a = args(&[OsStr::new("--output"), output.as_os_str()]);
        a.extend(extra.iter().map(OsString::from));
        let out = run_game(&a);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains(needle), "{needle:?} no esta en {stderr:?}");
        assert!(!output.exists());
    }
    // Salida que no es PNG, y alias de la salida con la textura real del fixture.
    let (model, texture) = write_fixture(&dir);
    let before = std::fs::read(&texture).unwrap();
    let jpg = dir.join("salida.jpg");
    let out = run_game(&creature_args(&model, &texture, &jpg, &[solar]));
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--output debe terminar en .png"));
    assert!(!jpg.exists());
    let out = run_game(&creature_args(&model, &texture, &texture, &[solar]));
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("coincide con la entrada"));
    assert_eq!(std::fs::read(&texture).unwrap(), before);
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--creature-solar-reveal"), "{help}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_flag_alone_starts_the_solar_game_and_redundant_flags_change_nothing() {
    let dir = temp_dir("cli-start");
    let (model, texture) = write_fixture(&dir);
    let (alone, redundant) = (dir.join("solo.png"), dir.join("redundante.png"));
    let solar = "--creature-solar-reveal";
    let out = run_game(&creature_args(
        &model,
        &texture,
        &alone,
        &[solar, "--debug-oxygen", "0.28"],
    ));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("Sol con ojo"), "{stdout}");
    let redundant_flags = [
        "--creature-reveal-stages",
        "--creature-eye-follow",
        "--creature-eye-compare",
        "--creature-horizontal-follow",
        solar,
        "--debug-oxygen",
        "0.28",
    ];
    let out = run_game(&creature_args(
        &model,
        &texture,
        &redundant,
        &redundant_flags,
    ));
    assert_eq!(out.status.code(), Some(0));
    let bytes = std::fs::read(&alone).unwrap();
    assert_eq!(bytes, std::fs::read(&redundant).unwrap());
    // El PNG headless es el cuadro de arranque del juego armado con la API (sin overlay ni
    // paneles), y no es el cuadro de la criatura de demostracion sin el flag.
    let (world, game) = game_world(W, H, 0.28);
    let (frame, _) = game_frame(&game, &world);
    let (w, h, pixels) = decode(&bytes);
    assert_eq!((w, h), (W, H));
    assert!(pixels == frame.pixels, "el PNG no es el cuadro solar");
    let plain = dir.join("sin-flag.png");
    let out = run_game(&creature_args(
        &model,
        &texture,
        &plain,
        &["--debug-oxygen", "0.28"],
    ));
    assert_eq!(out.status.code(), Some(0));
    let mut demo = new_world(W, H);
    demo.debug_oxygen01 = 0.28;
    demo.set_creature(creature(CreaturePlacement::demo()))
        .unwrap();
    let (_, _, plain_pixels) = decode(&std::fs::read(&plain).unwrap());
    assert!(
        plain_pixels == base_frame(&demo).pixels,
        "sin el flag todo es como antes"
    );
    assert!(plain_pixels != pixels);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn library_options(creature: Option<CreatureOptions>, output: &Path) -> GameOptions {
    GameOptions {
        width: 320,
        height: 180,
        headless: true,
        frame_limit: None,
        output: output.to_path_buf(),
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature,
        creature_reveal: false,
        creature_reveal_stages: false,
        creature_eye_follow: false,
        debug_oxygen: 0.28,
    }
}

#[test]
fn the_library_entry_point_has_the_command_line_guards_and_the_same_frame() {
    let dir = temp_dir("library");
    let (no_model, no_texture, output) = (
        dir.join("no.obj"),
        dir.join("no.png"),
        dir.join("salida.png"),
    );
    let requested = |near: bool| {
        CreatureOptions::from_flags(
            Some(no_model.clone()),
            Some(no_texture.clone()),
            near,
            None,
            false,
        )
        .unwrap()
    };
    for (options, needle) in [
        (
            library_options(None, &output),
            "--creature-solar-reveal necesita --creature-model y --creature-texture",
        ),
        (
            GameOptions {
                creature_reveal: true,
                ..library_options(requested(false), &output)
            },
            "--creature-reveal son incompatibles",
        ),
        (
            GameOptions {
                near: Some(0),
                ..library_options(requested(false), &output)
            },
            "--near son incompatibles",
        ),
        (
            library_options(requested(true), &output),
            "--near-creature son incompatibles",
        ),
        (
            library_options(requested(false), &dir.join("salida.bmp")),
            "--output debe terminar en .png",
        ),
    ] {
        let error = run_game_with_solar_reveal(options).expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
        assert!(!output.exists() && !no_model.exists() && !no_texture.exists());
    }
    let (model, texture) = write_fixture(&dir);
    let before = std::fs::read(&texture).unwrap();
    let real = |out: &Path| {
        let creature = CreatureOptions::from_flags(
            Some(model.clone()),
            Some(texture.clone()),
            false,
            None,
            false,
        )
        .unwrap();
        library_options(creature, out)
    };
    let error = run_game_with_solar_reveal(real(&texture)).expect_err("alias");
    assert!(error.contains("coincide con la entrada"), "{error}");
    assert_eq!(std::fs::read(&texture).unwrap(), before);
    // Etapas y mirada implicitas (aqui en false): el mismo cuadro que la CLI.
    run_game_with_solar_reveal(real(&output)).unwrap();
    let (world, game) = game_world(W, H, 0.28);
    let (frame, _) = game_frame(&game, &world);
    let (_, _, pixels) = decode(&std::fs::read(&output).unwrap());
    assert!(pixels == frame.pixels);
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- G7: asset real de Charlie (solo lectura, rutas por entorno; a mano con `--ignored`) ---

fn real_asset() -> (PathBuf, PathBuf) {
    let obj = PathBuf::from(std::env::var_os("UMBRAL_REAL_CREATURE_OBJ").expect("ruta del OBJ"));
    let png = PathBuf::from(
        std::env::var_os("UMBRAL_REAL_CREATURE_TEXTURE").expect("ruta de la textura"),
    );
    (obj, png)
}

/// Compara el binario (1280x720, solo el flag, O2 de la matriz) con los PNG aprobados del
/// preview 03 (`frontal320-o2-XXX.png` en `UMBRAL_SOLAR_PREVIEW03_DIR`, solo lectura). Escribe
/// sus salidas en una carpeta temporal propia.
#[test]
#[ignore = "asset real privado y PNG aprobados: rutas por variables de entorno"]
fn the_real_asset_game_start_is_the_approved_frontal_preview_at_every_level() {
    let (obj, png) = real_asset();
    let approved = PathBuf::from(
        std::env::var_os("UMBRAL_SOLAR_PREVIEW03_DIR").expect("carpeta del preview 03"),
    );
    let dir = temp_dir("real-oracles");
    for oxygen in LEVELS {
        let name = format!("frontal320-o2-{:03}.png", (oxygen * 100.0).round());
        let output = dir.join(&name);
        let o = OsStr::new;
        let level = oxygen.to_string();
        let out = run_game(&args(&[
            o("--creature-model"),
            obj.as_os_str(),
            o("--creature-texture"),
            png.as_os_str(),
            o("--creature-solar-reveal"),
            o("--debug-oxygen"),
            o(&level),
            o("--output"),
            output.as_os_str(),
        ]));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let ours = std::fs::read(&output).unwrap();
        let reference = std::fs::read(approved.join(&name)).unwrap();
        let (w, h, pixels) = decode(&ours);
        let (_, _, expected) = decode(&reference);
        let differing = pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
        println!("REAL_ORACLE {name}: {w}x{h}, pixeles distintos {differing}");
        assert_eq!((w, h), (1280, 720));
        assert_eq!(differing, 0, "{name}");
        assert_eq!(ours, reference, "{name}: bytes del PNG");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[ignore = "asset real privado: rutas por variables de entorno"]
fn the_real_asset_flies_tracks_and_reveals_in_the_solar_game_and_keeps_its_arrays() {
    let (obj, png) = real_asset();
    let (width, height) = (640, 360);
    let mut world = new_world(width, height);
    world.debug_oxygen01 = 0.35;
    let placement = solar_creature_placement(&world).unwrap();
    let mut game = SolarGame::start(
        &mut world,
        StaticCreature::load(&obj, &png, placement).unwrap(),
    )
    .unwrap();
    let skin = world.creature_eye_skin().unwrap() as *const _;
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    let mut base = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    let mut previous = world.solar_reveal_alphas().unwrap().sun;
    for frame in 0..240 {
        let less = frame % 30 == 0 && frame > 0 && frame <= 90;
        let more = frame % 30 == 0 && frame > 150;
        world.debug_oxygen01 = step_debug_oxygen(world.debug_oxygen01, more, less);
        if frame == 120 {
            game.apply(&mut world, OBLIQUE).unwrap();
        }
        let input = ShipInput {
            thrust: 0.3,
            yaw: 0.5,
            ..ShipInput::default()
        };
        if frame > 0 {
            advance_frame(&mut world, &input, DT);
        }
        if frame < 3 || frame % 40 == 0 {
            game.render(&world, &mut fb).unwrap();
            let mut solar = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
            world.render_solar_reveal_preview(&mut solar).unwrap();
            assert!(fb.pixels == solar.pixels, "cuadro {frame}");
            if world.solar_reveal_alphas().unwrap().eye > 0.0 {
                world.render(&mut base);
                assert!(fb.pixels != base.pixels, "cuadro {frame}");
            }
        }
        let sun = world.solar_reveal_alphas().unwrap().sun;
        assert!((sun - previous).abs() < 0.2, "cuadro {frame}");
        previous = sun;
    }
    let follow = world.creature_horizontal_follow().unwrap();
    println!(
        "REAL_FLIGHT: O2 {:.2}, sol {:.3}, cuerpo {:.2} grados, objetivo {:?}",
        world.debug_oxygen01,
        previous,
        follow.yaw().to_degrees(),
        follow.goal().map(f64::to_degrees)
    );
    assert!(follow.yaw() > 1f64.to_radians());
    assert!(std::ptr::eq(world.creature_eye_skin().unwrap(), skin));
    let fresh = load_model(&obj).unwrap();
    let model = world.creature().unwrap().model();
    assert!(model.vertices == fresh.vertices && model.uvs == fresh.uvs);
    assert!(model.normals == fresh.normals && model.bounds_center == fresh.bounds_center);
}
