//! Preview solar (candidata 03): curva del Sol visible, `World` opt-in y compositor de la pareja
//! central. Fixtures sinteticos (sin assets privados).

use umbral::visual::{
    solar_reveal_preview::*,
    staged_creature_reveal::{StagedRevealSettings, staged_alphas, staged_target},
};

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn sun_at_oxygen(oxygen: f64, settings: &StagedRevealSettings) -> f64 {
    solar_alpha(staged_target(oxygen, settings), settings)
}

// --- S1: curva del Sol visible (pura) ---

#[test]
fn the_sun_stays_whole_until_the_eye_is_complete_and_is_gone_at_the_hint_not_one_minus_body() {
    let s = StagedRevealSettings::default();
    for oxygen in [1.0, 0.75, 0.5, 0.45, 0.40, 0.35, 0.30] {
        assert_eq!(sun_at_oxygen(oxygen, &s), 1.0, "{oxygen}");
    }
    for oxygen in [0.25, 0.20, 0.15, 0.05, 0.0] {
        assert_eq!(sun_at_oxygen(oxygen, &s), 0.0, "{oxygen}");
    }
    for oxygen in [0.29, 0.28, 0.27, 0.26] {
        let sun = sun_at_oxygen(oxygen, &s);
        assert!(sun > 0.0 && sun < 1.0, "{oxygen}: {sun}");
    }
    // El 50 % exacto sigue con el ojo en 0 (curva aprobada sin cambios).
    assert_eq!(staged_alphas(staged_target(0.5, &s), &s), (0.0, 0.0));
    // Al 25 % el cuerpo es el contorno 0.02 pero el Sol ya no esta: no es `1 - cuerpo`.
    let (eye, body) = staged_alphas(staged_target(0.25, &s), &s);
    assert_eq!(eye, 1.0);
    assert!(close(body, 0.02, 1e-12));
    assert_eq!(sun_at_oxygen(0.25, &s), 0.0);
}

#[test]
fn the_sun_curve_is_monotone_finite_bounded_continuous_and_defensive() {
    let s = StagedRevealSettings::default();
    let steps = 20_000;
    let mut previous = solar_alpha(0.0, &s);
    assert_eq!(previous, 1.0);
    for i in 1..=steps {
        let p = i as f64 / steps as f64;
        let sun = solar_alpha(p, &s);
        assert!(sun.is_finite() && (0.0..=1.0).contains(&sun), "{p}: {sun}");
        assert!(sun <= previous, "no monotona en {p}: {previous} -> {sun}");
        // Continua: con smoothstep la pendiente maxima es 1.5 / (p_contorno - p_ojo).
        assert!(
            previous - sun <= 1.5 / 0.1111 / steps as f64 + 1e-12,
            "salto en {p}"
        );
        previous = sun;
    }
    assert_eq!(previous, 0.0);
    // Progreso NaN se trata como 0 (como las etapas: ojo oculto, Sol normal); fuera de rango se
    // recorta.
    assert_eq!(solar_alpha(f64::NAN, &s), 1.0);
    assert_eq!(solar_alpha(-3.0, &s), 1.0);
    assert_eq!(solar_alpha(f64::NEG_INFINITY, &s), 1.0);
    assert_eq!(solar_alpha(7.0, &s), 0.0);
    assert_eq!(solar_alpha(f64::INFINITY, &s), 0.0);
}

#[test]
fn while_the_sun_fades_the_eye_is_whole_and_while_the_eye_grows_the_sun_is_whole() {
    let s = StagedRevealSettings::default();
    for i in 0..=10_000 {
        let p = i as f64 / 10_000.0;
        let a = solar_reveal_alphas(p, &s);
        assert_eq!((a.eye, a.body), staged_alphas(p, &s), "{p}");
        assert_eq!(a.sun, solar_alpha(p, &s));
        if a.sun < 1.0 {
            assert_eq!(a.eye, 1.0, "Sol fundiendose con el ojo parcial en {p}");
        }
        if a.eye < 1.0 {
            assert_eq!(a.sun, 1.0, "ojo creciendo sin el Sol entero en {p}");
            assert_eq!(a.body, 0.0);
        }
        if a.body > 0.0 {
            assert_eq!(a.eye, 1.0);
        }
    }
}

#[test]
fn the_breakpoints_come_from_the_validated_stage_settings() {
    let s = StagedRevealSettings::new(0.8, 0.6, 0.4, 0.1, 0.0, 2.0, 0.7).unwrap();
    assert_eq!(sun_at_oxygen(0.6, &s), 1.0);
    assert_eq!(sun_at_oxygen(0.4, &s), 0.0);
    let half = sun_at_oxygen(0.5, &s);
    assert!(close(half, 0.5, 1e-12), "{half}");
    let (eye, hint) = solar_breakpoints(&s);
    assert!(close(eye, 0.25, 1e-15) && close(hint, 0.5, 1e-15));
    assert_eq!(staged_alphas(eye, &s).0, 1.0);
    assert!(close(staged_alphas(hint, &s).1, 0.1, 1e-12));
}

// --- Fixtures sinteticos: la cupula calibrable de las pruebas de la mirada ---

use umbral::{
    assets::{
        obj::{Model, load_model, parse_obj},
        ship_model_path,
    },
    math::Vec3,
    renderer::{framebuffer::Framebuffer, stats::RenderStats, texture::Texture},
    scene::world::{SPACE_COLOR, World},
    visual::{
        creature_eye_comparison::EyeTarget,
        creature_eye_follow::EyeFollowSettings,
        staged_creature_reveal::EyeRegion,
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const W: usize = 320;
const H: usize = 180;
/// Radio de la criatura sintetica: su ojo (apice de la cupula) queda DENTRO de la esfera del
/// Sol (radio 60), como el ojo del asset real con la escala 80.
const CREATURE_RADIUS: f64 = 30.0;

/// Cupula de radio 1 con apice +Z, anillos de 10 grados y 16 segmentos: con `dome_region` las
/// bandas 0-4 son ojo y las 5-7 cuerpo.
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

fn dome() -> Model {
    parse_obj(&dome_source()).unwrap()
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

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

fn creature_at(center: Vec3) -> StaticCreature {
    StaticCreature::new(
        dome(),
        texture(),
        CreaturePlacement::new(center, CREATURE_RADIUS, 0.0).unwrap(),
    )
    .unwrap()
}

/// Criatura con etapas, mirada a la camara y seguimiento 02 (sin el preview todavia).
fn staged_world_at(center: Vec3) -> World {
    let mut world = new_world();
    world.set_creature(creature_at(center)).unwrap();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
    world
        .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
        .unwrap();
    world.enable_creature_horizontal_follow().unwrap();
    world
}

/// Preview activo, camara frontal a ~220 unidades del Sol y el O2 pedido (sin transicion).
fn solar_world(light: SolarRevealLight, oxygen: f64) -> World {
    let mut world = staged_world_at(Vec3::ZERO);
    world.enable_solar_reveal_preview(light).unwrap();
    world.place_solar_reveal_camera(0.0, 200.0, 0.0).unwrap();
    with_oxygen(world, oxygen)
}

fn with_oxygen(mut world: World, oxygen: f64) -> World {
    world.debug_oxygen01 = oxygen;
    world.snap_creature_reveal();
    world
}

fn base(world: &World) -> (Framebuffer, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb, stats)
}

fn preview(world: &World) -> (Framebuffer, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render_solar_reveal_preview(&mut fb).unwrap();
    (fb, stats)
}

// --- S2: World opt-in, progreso mostrado, pose de perfil y extremo base ---

#[test]
fn the_preview_needs_stages_a_creature_and_the_creature_anchored_in_the_sun() {
    let light = SolarRevealLight::Camera;
    assert!(new_world().enable_solar_reveal_preview(light).is_err());
    let mut away = staged_world_at(Vec3::new(0.0, 0.0, -900.0));
    assert!(away.enable_solar_reveal_preview(light).is_err());
    assert_eq!(away.solar_reveal_alphas(), None);
    let mut empty = staged_world_at(Vec3::ZERO);
    empty.clear_creature();
    assert!(empty.enable_solar_reveal_preview(light).is_err());
    let mut sunless = staged_world_at(Vec3::ZERO);
    sunless.system.bodies.remove(0);
    assert!(sunless.enable_solar_reveal_preview(light).is_err());
    // Activarlo no cambia el cuadro de siempre ni el sistema solar.
    let mut world = staged_world_at(Vec3::ZERO);
    let before = base(&world).0.pixels;
    world.enable_solar_reveal_preview(light).unwrap();
    assert!(base(&world).0.pixels == before);
    assert_eq!(world.system.bodies.len(), 4);
    assert_eq!(world.system.light_position(), Vec3::ZERO);
    assert!(world.solar_reveal_alphas().is_some());
    world.disable_solar_reveal_preview();
    assert_eq!(world.solar_reveal_alphas(), None);
}

#[test]
fn the_profile_pose_looks_at_the_sun_from_the_asked_side_and_distance() {
    for (azimuth, distance, height) in [(0.0, 298.0, 0.0), (1.2, 578.0, 60.0)] {
        let mut world = solar_world(SolarRevealLight::Camera, 1.0);
        world
            .place_solar_reveal_camera(azimuth, distance, height)
            .unwrap();
        let ship = world.ship.position;
        let expected = Vec3::new(azimuth.sin(), 0.0, azimuth.cos()) * distance + Vec3::Y * height;
        assert!((ship - expected).length() < 1e-9, "{ship:?}");
        let to_sun = (Vec3::ZERO - world.camera.position).normalized();
        assert!(world.camera.forward().dot(to_sun) > 0.99);
        let camera_distance = world.camera.position.length();
        assert!(camera_distance > distance && camera_distance < distance + 30.0);
    }
    let mut world = solar_world(SolarRevealLight::Camera, 1.0);
    for bad in [
        (0.0, 60.0, 0.0),
        (f64::NAN, 300.0, 0.0),
        (0.0, 300.0, f64::INFINITY),
    ] {
        assert!(
            world
                .place_solar_reveal_camera(bad.0, bad.1, bad.2)
                .is_err()
        );
    }
}

#[test]
fn the_alphas_follow_the_shown_progress_with_real_seconds_and_reverse_with_oxygen() {
    let mut world = solar_world(SolarRevealLight::Camera, 1.0);
    let settings = StagedRevealSettings::default();
    let (eye_point, hint_point) = solar_breakpoints(&settings);
    let slope = 1.5 / (hint_point - eye_point);
    let first = world.solar_reveal_alphas().unwrap();
    assert_eq!(
        (first.progress, first.sun, first.eye, first.body),
        (0.0, 1.0, 0.0, 0.0)
    );
    // El O2 cae: en el mismo instante nada cambia (no es el O2 crudo).
    world.debug_oxygen01 = 0.2;
    assert_eq!(world.solar_reveal_alphas().unwrap(), first);
    let mut previous = first;
    let mut saw_fade = false;
    for (oxygen, falling) in [(0.2, true), (1.0, false)] {
        world.debug_oxygen01 = oxygen;
        for _ in 0..900 {
            world.update_creature_reveal(1.0 / 60.0);
            let a = world.solar_reveal_alphas().unwrap();
            assert_eq!(a.progress, world.staged_creature_reveal().unwrap().shown());
            assert!(a.sun.is_finite() && (0.0..=1.0).contains(&a.sun));
            if falling {
                assert!(a.sun <= previous.sun);
            } else {
                assert!(a.sun >= previous.sun);
            }
            // El Sol solo cambia tan rapido como el progreso mostrado.
            assert!(
                (a.sun - previous.sun).abs()
                    <= slope * (a.progress - previous.progress).abs() + 1e-12
            );
            saw_fade |= a.sun > 0.0 && a.sun < 1.0;
            if a.sun < 1.0 {
                assert_eq!(a.eye, 1.0);
            }
            previous = a;
        }
        assert_eq!(previous.sun, if falling { 0.0 } else { 1.0 });
    }
    assert!(saw_fade, "ninguna muestra en el fundido 30-25");
    assert_eq!(
        (previous.progress, previous.eye, previous.body),
        (0.0, 0.0, 0.0)
    );
}

#[test]
fn with_the_eye_hidden_the_preview_is_exactly_the_base_frame_and_the_plain_sun() {
    for oxygen in [1.0, 0.5] {
        let world = solar_world(SolarRevealLight::Camera, oxygen);
        assert_eq!(world.solar_reveal_alphas().unwrap().eye, 0.0);
        let (fb, stats) = preview(&world);
        let (reference, reference_stats) = base(&world);
        assert!(fb.pixels == reference.pixels, "{oxygen}");
        assert_eq!(stats, reference_stats);
        // Igual al Sol sin criatura alguna.
        let mut plain = with_oxygen(staged_world_at(Vec3::ZERO), oxygen);
        plain.clear_creature();
        plain.ship = world.ship;
        plain.camera = world.camera;
        assert!(base(&plain).0.pixels == fb.pixels, "{oxygen}");
    }
}

// --- S3: pareja central (Sol + criatura) con el pipeline real ---

use umbral::visual::creature_reveal::blend_rgb8;

/// El mismo mundo sin la representacion del Sol (despues de colocar la camara): referencia
/// de "Sol 0" con la ruta de etapas aprobada de siempre (`render`).
fn without_sun(mut world: World) -> World {
    world.system.bodies.remove(0);
    world
}

fn without_creature(mut world: World) -> World {
    world.clear_creature();
    world
}

fn differs(a: &[u32], b: &[u32]) -> Vec<bool> {
    a.iter().zip(b).map(|(x, y)| x != y).collect()
}

/// Pixeles del ojo (O2 30 %: ojo 1, cuerpo 0) sin Sol: donde el ojo cambia el fondo.
fn eye_mask_and_colors(light: SolarRevealLight) -> (Vec<bool>, Vec<u32>) {
    let eye = base(&without_sun(solar_world(light, 0.30))).0.pixels;
    let bg = base(&without_sun(without_creature(solar_world(light, 0.30))))
        .0
        .pixels;
    (differs(&eye, &bg), eye)
}

#[test]
fn at_and_below_the_hint_the_sun_leaves_no_rgb_or_depth_and_the_pair_is_the_approved_path() {
    for oxygen in [0.25, 0.15, 0.05, 0.0] {
        let world = solar_world(SolarRevealLight::World, oxygen);
        assert_eq!(world.solar_reveal_alphas().unwrap().sun, 0.0);
        let (fb, stats) = preview(&world);
        let (reference, reference_stats) =
            base(&without_sun(solar_world(SolarRevealLight::World, oxygen)));
        assert!(fb.pixels == reference.pixels, "RGB {oxygen}");
        assert_eq!(
            (stats.objects_submitted, stats.objects_culled),
            (
                reference_stats.objects_submitted,
                reference_stats.objects_culled
            )
        );
        // Sin z fantasma: la profundidad responde igual que sin Sol en toda la imagen.
        for y in (0..H).step_by(3) {
            for x in (0..W).step_by(3) {
                for depth in [-0.5, 0.0, 0.5, 0.9, 0.99, 0.999_999] {
                    let (x, y) = (x as isize, y as isize);
                    assert_eq!(
                        fb.depth_passes(x, y, depth),
                        reference.depth_passes(x, y, depth),
                        "z {oxygen} ({x}, {y}) {depth}"
                    );
                }
            }
        }
        // El Sol sigue en el sistema (solo se oculta su representacion).
        assert_eq!(world.system.bodies.len(), 4);
        assert_eq!(world.system.light_position(), Vec3::ZERO);
    }
}

#[test]
fn the_real_eye_reads_in_front_of_the_sun_at_the_same_place_and_colors_as_after_the_cut() {
    let light = SolarRevealLight::World;
    let (mask, eye) = eye_mask_and_colors(light);
    assert!(mask.iter().filter(|&&m| m).count() > 20);
    let plain_sun = base(&without_creature(solar_world(light, 0.30))).0.pixels;
    // Con el pipeline normal la esfera solar tapa el ojo entero (esta dentro de ella).
    assert!(base(&solar_world(light, 0.30)).0.pixels == plain_sun);
    for oxygen in [0.30, 0.40, 0.45] {
        let world = solar_world(light, oxygen);
        let a = world.solar_reveal_alphas().unwrap();
        assert_eq!((a.sun, a.body), (1.0, 0.0));
        assert!(a.eye > 0.0);
        let shown = preview(&world).0.pixels;
        for i in 0..W * H {
            let expected = if !mask[i] {
                plain_sun[i]
            } else if a.eye >= 1.0 {
                eye[i]
            } else {
                blend_rgb8(plain_sun[i], eye[i], a.eye)
            };
            assert_eq!(shown[i], expected, "{oxygen} pixel {i}");
        }
    }
}

/// Cuadrado (dos triangulos) en espacio del OBJ de la nave, `ahead` unidades por delante en
/// el eje de su frente y cubriendo `x` en `[x0, x1]`; de cara a la camara de persecucion (la
/// nave descarta caras de espaldas).
fn quad_ship(ahead: f64, x0: f64, x1: f64) -> Model {
    parse_obj(&format!(
        "v {x0} -80 {ahead}\nv {x1} -80 {ahead}\nv {x1} 80 {ahead}\nv {x0} 80 {ahead}\nf 1 3 2\nf 1 4 3\n"
    ))
    .unwrap()
}

#[test]
fn ship_like_quads_and_a_real_planet_in_front_hide_the_pair_with_the_world_depth() {
    let light = SolarRevealLight::World;
    let (mask, eye) = eye_mask_and_colors(light);
    // Quad delante de la camara tapando media imagen (y parte del ojo).
    let quad = |world: World, ahead| {
        let mut world = world;
        world.ship_model = quad_ship(ahead, -150.0, 0.0);
        world
    };
    let shown = preview(&quad(solar_world(light, 0.30), 12.0)).0.pixels;
    let plain = base(&quad(without_creature(solar_world(light, 0.30)), 12.0))
        .0
        .pixels;
    let hidden = base(&quad(without_creature(solar_world(light, 0.30)), -5000.0))
        .0
        .pixels;
    let covered = differs(&plain, &hidden);
    let both = (0..W * H).filter(|&i| covered[i] && mask[i]).count();
    let free = (0..W * H).filter(|&i| !covered[i] && mask[i]).count();
    assert!(
        both > 0 && free > 0,
        "el quad debe tapar parte del ojo: {both} {free}"
    );
    for i in 0..W * H {
        if covered[i] {
            assert_eq!(shown[i], plain[i], "quad {i}");
        } else if mask[i] {
            assert_eq!(shown[i], eye[i], "ojo libre {i}");
        }
    }
    // Planeta real (Antena) entre la camara y el Sol.
    let at_transit = |mut world: World| {
        world.time = (std::f64::consts::FRAC_PI_2 - 0.6) / 0.060;
        world
    };
    let mut far = solar_world(light, 0.30);
    far.place_solar_reveal_camera(0.0, 300.0, 0.0).unwrap();
    let mask_far = {
        let mut eye_world = without_sun(solar_world(light, 0.30));
        let mut bg_world = without_sun(without_creature(solar_world(light, 0.30)));
        for w in [&mut eye_world, &mut bg_world] {
            w.ship = far.ship;
            w.camera = far.camera;
            w.update_creature_eye_follow(1000.0);
        }
        differs(&base(&eye_world).0.pixels, &base(&bg_world).0.pixels)
    };
    far.update_creature_eye_follow(1000.0);
    let far = at_transit(far);
    let shown = preview(&far).0.pixels;
    let mut plain_world = at_transit(without_creature(solar_world(light, 0.30)));
    plain_world.ship = far.ship;
    plain_world.camera = far.camera;
    let plain = base(&plain_world).0.pixels;
    let mut no_planet = at_transit(without_creature(solar_world(light, 0.30)));
    no_planet.ship = far.ship;
    no_planet.camera = far.camera;
    no_planet.system.bodies.remove(1);
    let planet = differs(&plain, &base(&no_planet).0.pixels);
    let behind = (0..W * H).filter(|&i| planet[i] && mask_far[i]).count();
    assert!(behind > 0, "el planeta debe pasar delante del ojo");
    for i in 0..W * H {
        if planet[i] {
            assert_eq!(shown[i], plain[i], "planeta {i}");
        }
    }
}

#[test]
fn the_partial_sun_blends_over_the_real_background_not_toward_black() {
    let light = SolarRevealLight::World;
    let world = solar_world(light, 0.28);
    let a = world.solar_reveal_alphas().unwrap();
    assert!(a.sun > 0.0 && a.sun < 1.0 && a.eye == 1.0, "{a:?}");
    let shown = preview(&world).0.pixels;
    let plain_sun = base(&without_creature(solar_world(light, 0.28))).0.pixels;
    let bg = base(&without_sun(without_creature(solar_world(light, 0.28))))
        .0
        .pixels;
    let creature = differs(
        &base(&without_sun(solar_world(light, 0.0))).0.pixels,
        &base(&without_sun(without_creature(solar_world(light, 0.0))))
            .0
            .pixels,
    );
    let sun = differs(&plain_sun, &bg);
    let mut ring = 0;
    for i in 0..W * H {
        if creature[i] {
            continue;
        }
        if sun[i] {
            ring += 1;
            assert_eq!(
                shown[i],
                blend_rgb8(bg[i], plain_sun[i], a.sun),
                "anillo {i}"
            );
        } else {
            assert_eq!(shown[i], bg[i], "fuera {i}");
        }
    }
    assert!(ring > 100, "{ring}");
}

#[test]
fn the_local_light_only_changes_the_creature_and_buffers_are_reused() {
    let (mask, _) = eye_mask_and_colors(SolarRevealLight::World);
    let world_light = preview(&solar_world(SolarRevealLight::World, 0.30))
        .0
        .pixels;
    let camera = solar_world(SolarRevealLight::Camera, 0.30);
    let (fb, stats) = preview(&camera);
    let mut changed = 0;
    for i in 0..W * H {
        if mask[i] {
            changed += usize::from(fb.pixels[i] != world_light[i]);
        } else {
            assert_eq!(fb.pixels[i], world_light[i], "fuera del ojo {i}");
        }
    }
    assert!(changed > 0);
    // Cuenta el trabajo real: cuatro cuerpos, la criatura y la nave.
    assert_eq!(stats.objects_submitted, 6);
    assert!(stats.vertices_processed > 0 && stats.fragments_evaluated > 0);
    let bytes = camera.solar_reveal_buffer_bytes();
    assert!(bytes > 0);
    for _ in 0..3 {
        preview(&camera);
        assert_eq!(camera.solar_reveal_buffer_bytes(), bytes);
    }
}

// --- V1: el Sol en cache del preview (revision independiente, caso 1) ---

use std::panic::{AssertUnwindSafe, catch_unwind};
use umbral::renderer::shader::ShaderKind;

/// Estado observable que una pose o un cuadro rechazados no deben tocar (Debug de f64 es
/// exacto: ida y vuelta).
fn observable(world: &World) -> String {
    format!(
        "{:?}",
        (
            world.ship,
            world.camera,
            world.staged_creature_reveal(),
            world.creature_eye_follow(),
            world.creature_horizontal_follow(),
            world.time,
            world.debug_oxygen01,
        )
    )
}

/// `Err` sin panico y sin efectos; el panico cuenta como fallo de la prueba, no se oculta.
fn rejects_without_effects(world: &mut World, pose: (f64, f64, f64), case: &str) {
    let before = observable(world);
    let result = catch_unwind(AssertUnwindSafe(|| {
        world.place_solar_reveal_camera(pose.0, pose.1, pose.2)
    }));
    match result {
        Ok(Err(_)) => {}
        Ok(Ok(())) => panic!("{case}: pose aceptada {pose:?}"),
        Err(_) => panic!("{case}: la pose {pose:?} entro en panico en vez de Err"),
    }
    assert_eq!(observable(world), before, "{case}: estado cambiado");
}

fn render_rejects(world: &World, case: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
        world.render_solar_reveal_preview(&mut fb).map(|_| ())
    }));
    assert!(
        matches!(result, Ok(Err(_))),
        "{case}: el cuadro solar debe dar Err sin panico"
    );
}

/// Caso y edicion del sistema solar despues de activar el preview.
type SystemEdit = (&'static str, fn(&mut World));

#[test]
fn a_cached_sun_that_is_gone_moved_or_no_longer_a_sun_is_rejected_without_effects() {
    let edits: [SystemEdit; 6] = [
        ("sin cuerpos", |w| w.system.bodies.clear()),
        ("Sol quitado (otro cuerpo en su indice)", |w| {
            w.system.bodies.remove(0);
        }),
        ("ya no es el sol animado", |w| {
            w.system.bodies[0].shader = ShaderKind::LitPlanet
        }),
        ("con orbita", |w| w.system.bodies[0].orbit.radius = 100.0),
        ("radio NaN", |w| w.system.bodies[0].body_radius = f64::NAN),
        ("radio 0", |w| w.system.bodies[0].body_radius = 0.0),
    ];
    for (case, edit) in edits {
        let mut world = solar_world(SolarRevealLight::Camera, 0.30);
        edit(&mut world);
        rejects_without_effects(&mut world, (0.0, 298.0, 0.0), case);
        render_rejects(&world, case);
    }
    // Al activar, el unico cuerpo sin orbita que no es el sol animado no sirve.
    let mut world = staged_world_at(Vec3::ZERO);
    world.system.bodies[0].shader = ShaderKind::LitPlanet;
    assert!(
        world
            .enable_solar_reveal_preview(SolarRevealLight::Camera)
            .is_err()
    );
    assert_eq!(world.solar_reveal_alphas(), None);
}

// --- V2: preflight de la pose compuesta (revision independiente, caso 2) ---

use umbral::math::Vec4;

fn finite_matrix(m: &umbral::math::Mat4) -> bool {
    m.m.iter().flatten().all(|v| v.is_finite())
}

#[test]
fn absurd_or_degenerate_profile_poses_are_rejected_before_touching_the_world() {
    // Entradas finitas cuya pose compuesta no sirve: desborde a infinito, direccion nula o
    // fuera del contrato de coordenadas.
    for pose in [
        (0.0, f64::MAX, 0.0),
        (0.0, 1e300, 0.0),
        (0.0, 298.0, 1e300),
        (0.0, 298.0, f64::MAX),
        (0.0, 298.0, -f64::MAX),
        (0.0, 2e6, 0.0),
        (0.0, 61.0, 2e6),
    ] {
        let mut world = solar_world(SolarRevealLight::Camera, 0.30);
        rejects_without_effects(&mut world, pose, "pose absurda");
    }
    // La camara resultante tambien tiene que ser usable.
    for (field, value) in [
        ("chase_distance", f64::NAN),
        ("chase_distance", f64::MAX),
        ("chase_height", f64::INFINITY),
        ("chase_height", -f64::MAX),
    ] {
        let mut world = solar_world(SolarRevealLight::Camera, 0.30);
        if field == "chase_distance" {
            world.chase_distance = value;
        } else {
            world.chase_height = value;
        }
        rejects_without_effects(&mut world, (0.0, 298.0, 0.0), field);
    }
}

#[test]
fn reasonable_poses_keep_the_exact_ship_and_chase_camera_and_stay_usable() {
    for (azimuth, distance, height) in [
        (0.0, 298.0, 0.0),
        (70.0_f64.to_radians(), 578.0, 60.0),
        (0.0, 61.0, 9e5),
        (-2.5, 61.0, -9e5),
    ] {
        let mut world = solar_world(SolarRevealLight::Camera, 0.30);
        world
            .place_solar_reveal_camera(azimuth, distance, height)
            .unwrap();
        // La misma nave que antes del preflight (bit a bit) y la camara de persecucion.
        let position = Vec3::new(azimuth.sin(), 0.0, azimuth.cos()) * distance + Vec3::Y * height;
        let expected =
            umbral::scene::ship::Ship::new(position, (Vec3::ZERO - position).normalized());
        assert_eq!(format!("{:?}", world.ship), format!("{expected:?}"));
        let basis = expected.orientation;
        let (back, up) = (basis.transform_dir(Vec3::Z), basis.transform_dir(Vec3::Y));
        let camera = world.camera;
        assert_eq!(
            camera.position,
            position + back * world.chase_distance + up * world.chase_height
        );
        assert_eq!(camera.target, position - back * 30.0);
        assert_eq!(camera.up, up);
        assert!(finite_matrix(&camera.view_projection()));
        let forward = camera.forward();
        assert!(forward.is_finite() && (forward.length() - 1.0).abs() < 1e-9);
        let center = camera
            .view_projection()
            .transform(Vec4::new(0.0, 0.0, 0.0, 1.0));
        assert!(center.w > 0.0, "el Sol queda delante de la camara");
    }
}

// --- V3: el compositor valida el framebuffer antes de reservar o tocar nada (caso 3) ---

use umbral::scene::camera::Camera;

fn test_camera() -> Camera {
    Camera::new(1.0, 1.0, 0.3, 100.0).unwrap()
}

/// `Err` sin panico, sin reservar buffers y sin tocar el cuadro.
fn compositor_rejects(fb: &mut Framebuffer, case: &str) {
    let mut compositor = SolarPairCompositor::new();
    let pixels = fb.pixels.clone();
    let result = catch_unwind(AssertUnwindSafe(|| {
        compositor.render(fb, &test_camera(), &[]).map(|_| ())
    }));
    assert!(
        matches!(result, Ok(Err(_))),
        "{case}: debe dar Err sin panico"
    );
    assert_eq!(
        compositor.buffer_bytes(),
        0,
        "{case}: reservo antes de validar"
    );
    assert!(fb.pixels == pixels, "{case}: toco el cuadro");
}

#[test]
fn the_compositor_rejects_incoherent_framebuffers_before_allocating() {
    for (width, height) in [(usize::MAX, 2), (2, 1), (1, 2), (0, 1), (1, 0)] {
        let mut fb = Framebuffer::new(1, 1, 7).unwrap();
        (fb.width, fb.height) = (width, height);
        compositor_rejects(&mut fb, &format!("{width}x{height} sobre 1 pixel"));
    }
    let mut fb = Framebuffer::new(2, 2, 7).unwrap();
    fb.pixels.truncate(1);
    compositor_rejects(&mut fb, "2x2 con 1 pixel");
}

// --- V3b (despues del GREEN): limites y buffers reutilizados frente a frescos ---

use umbral::renderer::framebuffer::MAX_PIXELS;

#[test]
fn the_pixel_limit_is_checked_before_any_reservation() {
    for (width, height) in [(MAX_PIXELS + 1, 1), (MAX_PIXELS, 1), (1, MAX_PIXELS)] {
        let mut fb = Framebuffer::new(1, 1, 7).unwrap();
        (fb.width, fb.height) = (width, height);
        compositor_rejects(&mut fb, &format!("{width}x{height} sobre 1 pixel"));
    }
}

/// Mismos colores y misma respuesta del z-buffer (rejilla de sondas) en dos cuadros.
fn same_frame(a: &Framebuffer, b: &Framebuffer, case: &str) {
    assert!(a.pixels == b.pixels, "{case}: RGB");
    for y in (0..a.height).step_by(3) {
        for x in (0..a.width).step_by(3) {
            for depth in [-0.5, 0.0, 0.5, 0.9, 0.99, 0.999_999] {
                let (x, y) = (x as isize, y as isize);
                assert_eq!(
                    a.depth_passes(x, y, depth),
                    b.depth_passes(x, y, depth),
                    "{case}: z ({x}, {y}) {depth}"
                );
            }
        }
    }
}

fn preview_sized(world: &World, size: (usize, usize)) -> Framebuffer {
    let mut fb = Framebuffer::new(size.0, size.1, SPACE_COLOR).unwrap();
    world.render_solar_reveal_preview(&mut fb).unwrap();
    fb
}

#[test]
fn reused_buffers_give_the_same_frame_as_fresh_ones_across_sizes_and_layers() {
    let light = SolarRevealLight::World;
    let mut reused = solar_world(light, 0.30);
    // Mismo numero de pixeles con otras dimensiones, cuadros sin compositor (ojo oculto) y
    // cambios de capas activas (Sol+ojo, las tres, sin Sol).
    let steps = [
        (0.30, (W, H)),
        (0.30, (H, W)),
        (1.0, (H, W)),
        (0.05, (W, H)),
        (0.28, (W, H)),
        (0.28, (160, 90)),
        (0.30, (W, H)),
    ];
    for (oxygen, size) in steps {
        reused = with_oxygen(reused, oxygen);
        let fresh = solar_world(light, oxygen);
        let case = format!("O2 {oxygen} {}x{}", size.0, size.1);
        same_frame(
            &preview_sized(&reused, size),
            &preview_sized(&fresh, size),
            &case,
        );
    }
    // Sin capas no toca el cuadro.
    let mut fb = Framebuffer::new(4, 3, 7).unwrap();
    let stats = SolarPairCompositor::new()
        .render(&mut fb, &test_camera(), &[])
        .unwrap();
    assert!(fb.pixels.iter().all(|&p| p == 7));
    assert_eq!(stats, RenderStats::default());
}
