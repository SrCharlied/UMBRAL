//! Parent RED: the pupil-only variant must affect the real combined image, not just metadata.
#[allow(dead_code)]
#[path = "fixtures/watching_stars_v1.rs"]
mod approved_preview_v1;
#[allow(dead_code)]
#[path = "../src/visual/watching_stars.rs"]
mod candidate;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
    visual::perception_game::apply_lucid_perception,
};

#[test]
fn low_oxygen_pupil_variant_changes_the_real_combined_image() {
    let (w, h) = (1280, 720);
    let mut world =
        World::new(load_model(&ship_model_path()).unwrap(), w as f64 / h as f64).unwrap();
    world.debug_oxygen01 = 0.25;
    let mut fb = Framebuffer::new(w, h, SPACE_COLOR).unwrap();
    world.render(&mut fb);
    let before =
        approved_preview_v1::render_watching_stars(&fb, &world.camera, world.ship.position, 0.25)
            .unwrap();
    let after =
        candidate::render_watching_stars(&fb, &world.camera, world.ship.position, 0.25).unwrap();
    let mut a = Framebuffer::new(w, h, SPACE_COLOR).unwrap();
    let mut b = Framebuffer::new(w, h, SPACE_COLOR).unwrap();
    a.pixels = before.pixels;
    b.pixels = after.pixels;
    apply_lucid_perception(&mut a, &world).unwrap();
    apply_lucid_perception(&mut b, &world).unwrap();
    assert!(
        a.pixels != b.pixels,
        "the pupil variant must change actual pixels AFTER accepted perception, not only the clean eye or metadata"
    );
}

// ---------------------------------------------------------------------------------------------
// Pruebas del worker (Bronya), preview02 «pupila reforzada progresiva». El prefijo de arriba es el
// tracer del padre, intacto. `approved_preview_v1` es la copia PADRE inmutable de preview01.
// ---------------------------------------------------------------------------------------------

#[test]
fn pupil_reinforcement_starts_below_half_oxygen_and_is_full_by_a_quarter() {
    for o in [1.0, 0.8, 0.65, 0.6, 0.55, 0.5] {
        assert_eq!(
            candidate::pupil_reinforcement(o).unwrap(),
            0.0,
            "exact v1 at {o}"
        );
    }
    for o in [0.25, 0.2, 0.1, 0.05, 0.0] {
        assert_eq!(
            candidate::pupil_reinforcement(o).unwrap(),
            1.0,
            "full at {o}"
        );
    }
    // Suave: arranca casi en 0 justo bajo el 50 %, sube sin saltos y sin retrocesos hasta el 25 %.
    let start = candidate::pupil_reinforcement(0.49).unwrap();
    assert!(start > 0.0 && start < 0.01, "gentle start at 49%: {start}");
    let mut last = 0.0;
    for k in (250..=500).rev() {
        let r = candidate::pupil_reinforcement(k as f64 / 1000.0).unwrap();
        assert!(
            r >= last && r - last < 0.01,
            "monotone and smooth at {k}/1000: {last} -> {r}"
        );
        last = r;
    }
    for o in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(candidate::pupil_reinforcement(o).is_err(), "{o}");
    }
}

const SIZE: (usize, usize) = (1280, 720);

/// Cuadro original de una vista del preview (como el example): `normal` (arranque), `near3`
/// (`--near 3`) o `sky` (solo las estrellas de `draw_stars` con la camara del arranque normal).
fn view(name: &str, oxygen01: f64) -> (World, Framebuffer) {
    let mut world = World::new(
        load_model(&ship_model_path()).unwrap(),
        SIZE.0 as f64 / SIZE.1 as f64,
    )
    .unwrap();
    if name == "near3" {
        world.place_near(3, 25.0).unwrap();
    }
    world.debug_oxygen01 = oxygen01;
    let mut fb = Framebuffer::new(SIZE.0, SIZE.1, SPACE_COLOR).unwrap();
    if name == "sky" {
        let vp = approved_preview_v1::sky_view_projection(&world.camera);
        for star in approved_preview_v1::star_catalog() {
            if let Some((x, y)) =
                approved_preview_v1::star_pixel(&vp, star.direction, SIZE.0, SIZE.1)
            {
                fb.write(x, y, 1.0, star.color);
            }
        }
    } else {
        world.render(&mut fb);
    }
    (world, fb)
}

/// Composicion aprobada: ojos sobre el cuadro y DESPUES la percepcion (nunca ojos encima).
fn combined(world: &World, frame: &Framebuffer, eyes: &[u32]) -> Vec<u32> {
    let mut fb = Framebuffer::new(frame.width, frame.height, SPACE_COLOR).unwrap();
    fb.pixels = eyes.to_vec();
    apply_lucid_perception(&mut fb, world).unwrap();
    fb.pixels
}

fn luma(p: u32) -> f64 {
    let c = |s: u32| f64::from((p >> s) & 0xff);
    0.299 * c(16) + 0.587 * c(8) + 0.114 * c(0)
}

fn index(x: isize, y: isize) -> Option<usize> {
    (x >= 0 && y >= 0 && (x as usize) < SIZE.0 && (y as usize) < SIZE.1)
        .then(|| y as usize * SIZE.0 + x as usize)
}

/// Centroide (px, desde el centro del pixel de la estrella) de `signal` en una ventana, proyectado
/// sobre la direccion de la mirada (positivo = hacia la nave). `None` sin masa.
fn centroid_along_gaze(
    eye: &candidate::ObserverEye,
    half: (isize, isize),
    signal: impl Fn(usize) -> f64,
) -> Option<f64> {
    let len = eye.pupil_offset[0].hypot(eye.pupil_offset[1]);
    let (mut mass, mut along) = (0.0, 0.0);
    for dy in -half.1..=half.1 {
        for dx in -half.0..=half.0 {
            if let Some(i) = index(eye.pixel.0 + dx, eye.pixel.1 + dy) {
                let s = signal(i);
                mass += s;
                along +=
                    s * (dx as f64 * eye.pupil_offset[0] + dy as f64 * eye.pupil_offset[1]) / len;
            }
        }
    }
    (mass > 0.0).then(|| along / mass)
}

/// Ojos abiertos del todo, con mirada desplazada y sin otro ojo cerca (ventana sin mezclas).
fn isolated_looking_eyes(eyes: &[candidate::ObserverEye]) -> Vec<&candidate::ObserverEye> {
    eyes.iter()
        .filter(|e| {
            e.weight == 1.0
                && e.pupil_offset[0].hypot(e.pupil_offset[1]) > 1.0
                && eyes.iter().all(|o| {
                    o.id == e.id
                        || (o.pixel.0 - e.pixel.0).abs() > 16
                        || (o.pixel.1 - e.pixel.1).abs() > 12
                })
        })
        .collect()
}

#[test]
fn reinforced_pupil_adds_darkness_inside_the_eye_toward_the_ship() {
    let (world, frame) = view("normal", 0.25);
    let ship = world.ship.position;
    let old =
        approved_preview_v1::render_watching_stars(&frame, &world.camera, ship, 0.25).unwrap();
    let new = candidate::render_watching_stars(&frame, &world.camera, ship, 0.25).unwrap();
    let darkness = |i: usize| luma(old.pixels[i]) - luma(new.pixels[i]);
    let total: f64 = (0..old.pixels.len()).map(darkness).sum();
    assert!(
        total > 0.0,
        "the reinforced pupil must add dark mass before the post: {total}"
    );
    let looking = isolated_looking_eyes(&new.report.eyes);
    let toward = looking
        .iter()
        .filter(|e| centroid_along_gaze(e, (4, 2), darkness).is_some_and(|c| c > 0.3))
        .count();
    assert!(
        looking.len() >= 40 && toward * 10 >= looking.len() * 9,
        "added darkness must sit toward the ship in >= 90% of looking eyes: {toward}/{}",
        looking.len()
    );
}

#[test]
fn after_accepted_perception_the_eye_light_leans_away_from_the_ship() {
    for name in ["normal", "sky"] {
        let (world, frame) = view(name, 0.25);
        let ship = world.ship.position;
        let old =
            approved_preview_v1::render_watching_stars(&frame, &world.camera, ship, 0.25).unwrap();
        let new = candidate::render_watching_stars(&frame, &world.camera, ship, 0.25).unwrap();
        let base = combined(&world, &frame, &frame.pixels);
        let a = combined(&world, &frame, &old.pixels);
        let b = combined(&world, &frame, &new.pixels);
        let looking = isolated_looking_eyes(&new.report.eyes);
        let (mut sum_old, mut sum_new, mut n) = (0.0, 0.0, 0usize);
        for e in &looking {
            let light = |p: &[u32]| {
                centroid_along_gaze(e, (7, 5), |i| (luma(p[i]) - luma(base[i])).max(0.0))
            };
            if let (Some(o), Some(c)) = (light(&a), light(&b)) {
                (sum_old, sum_new, n) = (sum_old + o, sum_new + c, n + 1);
            }
        }
        let (mean_old, mean_new) = (sum_old / n as f64, sum_new / n as f64);
        eprintln!(
            "{name}: {n} eyes, light centroid along gaze v1 {mean_old:+.3} px, new {mean_new:+.3} px"
        );
        assert!(n >= 20, "{name}: too few isolated eyes to measure: {n}");
        assert!(
            mean_new < mean_old - 0.25,
            "{name}: after the approved post the dark pupil must push the eye light away from the ship: v1 {mean_old:+.3} new {mean_new:+.3}"
        );
    }
}

// --- Guards de preservacion NACIDOS VERDES (born-green): pasan desde que se escribieron; su
// sensibilidad se demuestra con mutaciones en una copia desechable (worker-evidence/mutation_check.py).

const BELOW_HALF: [f64; 7] = [0.45, 0.4, 0.3, 0.25, 0.1, 0.05, 0.0];

fn both(
    world: &World,
    frame: &Framebuffer,
    oxygen01: f64,
) -> (approved_preview_v1::WatchingStars, candidate::WatchingStars) {
    let (cam, ship) = (&world.camera, world.ship.position);
    (
        approved_preview_v1::render_watching_stars(frame, cam, ship, oxygen01).unwrap(),
        candidate::render_watching_stars(frame, cam, ship, oxygen01).unwrap(),
    )
}

#[test]
fn born_green_at_or_above_half_oxygen_is_exactly_preview01() {
    for name in ["normal", "near3", "sky"] {
        for o in [1.0, 0.7, 0.65, 0.6, 0.55, 0.5] {
            let (world, frame) = view(name, o);
            let (old, new) = both(&world, &frame, o);
            assert!(
                old.pixels == new.pixels,
                "{name} {o}: pixels must be preview01 exactly"
            );
            assert_eq!(
                format!("{:?}", old.report),
                format!("{:?}", new.report),
                "{name} {o}"
            );
            assert!(
                combined(&world, &frame, &old.pixels) == combined(&world, &frame, &new.pixels),
                "{name} {o}: combined must be preview01 exactly"
            );
        }
    }
    // Barrido fino 50..70 % en el cielo (todas las estrellas a la vista de la camara normal).
    let (world, frame) = view("sky", 0.5);
    for k in 0..=40 {
        let o = 0.5 + k as f64 * 0.005;
        let (old, new) = both(&world, &frame, o);
        assert!(old.pixels == new.pixels, "sky sweep {o}");
    }
}

#[test]
fn born_green_population_gaze_and_id_metadata_match_preview01() {
    for name in ["normal", "near3", "sky"] {
        for o in BELOW_HALF {
            let (world, frame) = view(name, o);
            let (old, new) = both(&world, &frame, o);
            let (a, b) = (&old.report, &new.report);
            assert_eq!(
                format!(
                    "{:?}",
                    (a.oxygen01, a.density, &a.active_ids, &a.projected_ids)
                ),
                format!(
                    "{:?}",
                    (b.oxygen01, b.density, &b.active_ids, &b.projected_ids)
                ),
                "{name} {o}: population"
            );
            assert_eq!(
                format!("{:?}", (&a.occluded_ids, &a.shadowed_ids, a.ship_focus_px)),
                format!("{:?}", (&b.occluded_ids, &b.shadowed_ids, b.ship_focus_px)),
                "{name} {o}: occlusion and focus"
            );
            assert_eq!(
                a.eyes
                    .iter()
                    .map(|e| format!("{:?}", (e.id, e.rank, e.pixel, e.weight, e.pupil_offset)))
                    .collect::<Vec<_>>(),
                b.eyes
                    .iter()
                    .map(|e| format!("{:?}", (e.id, e.rank, e.pixel, e.weight, e.pupil_offset)))
                    .collect::<Vec<_>>(),
                "{name} {o}: eye IDs, ranks, pixels, opening weights and gaze offsets"
            );
        }
    }
}

/// Pixeles que cambia cada version sobre un cielo de color liso (sin geometria): soporte del ojo.
fn support_on(
    color: u32,
    world: &World,
    oxygen01: f64,
) -> (Vec<bool>, Vec<bool>, Vec<candidate::ObserverEye>) {
    let frame = Framebuffer::new(SIZE.0, SIZE.1, color).unwrap();
    let (old, new) = both(world, &frame, oxygen01);
    let mask = |p: &[u32]| p.iter().map(|&v| v != color).collect::<Vec<_>>();
    (mask(&old.pixels), mask(&new.pixels), new.report.eyes)
}

fn in_eye_box(e: &candidate::ObserverEye, i: usize) -> bool {
    let (x, y) = ((i % SIZE.0) as isize, (i / SIZE.0) as isize);
    (x - e.pixel.0).abs() <= 4 && (y - e.pixel.1).abs() <= 2
}

#[test]
fn born_green_outer_eye_support_is_invariant() {
    let (world, _) = view("normal", 0.25);
    for o in BELOW_HALF {
        // Sobre blanco cualquier pixel cubierto por un ojo abierto del todo cambia (alfa >= 1/16 y
        // todos los colores del ojo quedan >= 36 niveles bajo 255): ahi el soporte es exacto.
        let (old, new, eyes) = support_on(0xFF_FFFF, &world, o);
        for i in 0..old.len() {
            let mut boxes = eyes.iter().filter(|e| in_eye_box(e, i));
            let Some(first) = boxes.next() else {
                assert!(
                    !old[i] && !new[i],
                    "{o}: pixel {i} outside every 9x5 eye box changed"
                );
                continue;
            };
            if first.weight == 1.0 && boxes.next().is_none() {
                assert_eq!(
                    old[i], new[i],
                    "{o}: support of a fully open eye at pixel {i}"
                );
            }
        }
    }
    // En cuadros reales: lo que cambia frente a preview01 esta en cielo y dentro de un ojo dibujado.
    for name in ["normal", "near3"] {
        for o in [0.3, 0.1] {
            let (world, frame) = view(name, o);
            let (old, new) = both(&world, &frame, o);
            for i in (0..old.pixels.len()).filter(|&i| old.pixels[i] != new.pixels[i]) {
                let (x, y) = ((i % SIZE.0) as isize, (i / SIZE.0) as isize);
                assert!(
                    frame.depth_passes(x, y, candidate::SKY_DEPTH_PROBE),
                    "{name} {o}: {i} over geometry"
                );
                assert!(
                    new.report.eyes.iter().any(|e| in_eye_box(e, i)),
                    "{name} {o}: {i} outside eyes"
                );
            }
        }
    }
}

#[test]
fn born_green_sclera_and_opacity_outside_the_dark_core_are_bit_identical() {
    for name in ["normal", "near3", "sky"] {
        for o in BELOW_HALF {
            let (world, frame) = view(name, o);
            let (old, new) = both(&world, &frame, o);
            let mut compared = 0;
            for e in &new.report.eyes {
                for dy in -2..=2isize {
                    for dx in -4..=4isize {
                        // Distancia minima del cuadrado del pixel al centro de la pupila.
                        let gap = |d: isize, c: f64| (((d as f64 - c).abs()) - 0.5).max(0.0);
                        let near = gap(dx, e.pupil_offset[0]).hypot(gap(dy, e.pupil_offset[1]));
                        let Some(i) = index(e.pixel.0 + dx, e.pixel.1 + dy) else {
                            continue;
                        };
                        if near > candidate::REINFORCED_IRIS_RADIUS_PX + 1e-9 {
                            compared += 1;
                            assert_eq!(
                                old.pixels[i], new.pixels[i],
                                "{name} {o}: eye {} at ({dx},{dy})",
                                e.id
                            );
                        }
                    }
                }
            }
            assert!(
                new.report.eyes.is_empty() || compared > 0,
                "{name} {o}: nothing compared"
            );
        }
    }
}

#[test]
fn born_green_reinforcement_never_brightens_any_channel() {
    for name in ["normal", "near3", "sky"] {
        for o in BELOW_HALF {
            let (world, frame) = view(name, o);
            let (old, new) = both(&world, &frame, o);
            for (i, (&a, &b)) in old.pixels.iter().zip(&new.pixels).enumerate() {
                for s in [16, 8, 0] {
                    assert!(
                        (b >> s) & 0xff <= (a >> s) & 0xff,
                        "{name} {o}: pixel {i} brighter: {a:06x} -> {b:06x}"
                    );
                }
            }
        }
    }
}

#[test]
fn born_green_invalid_inputs_are_rejected_like_preview01() {
    let (world, _) = view("sky", 0.5);
    let (cam, ship) = (world.camera, world.ship.position);
    let sky = Framebuffer::new(64, 36, SPACE_COLOR).unwrap();
    let check = |fb: &Framebuffer,
                 cam: &umbral::scene::camera::Camera,
                 ship: umbral::math::Vec3,
                 o: f64| {
        let old = approved_preview_v1::render_watching_stars(fb, cam, ship, o).map(|_| ());
        let new = candidate::render_watching_stars(fb, cam, ship, o).map(|_| ());
        assert!(old.is_err(), "preview01 rejects this case ({o})");
        assert_eq!(old, new, "same rejection as preview01 ({o})");
    };
    for o in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        check(&sky, &cam, ship, o);
        assert_eq!(
            approved_preview_v1::observer_weights(o).map(|_| ()),
            candidate::pupil_reinforcement(o).map(|_| ())
        );
    }
    let mut bad_ship = ship;
    bad_ship.y = f64::NAN;
    check(&sky, &cam, bad_ship, 0.2);
    let mut bad = cam;
    bad.position.x = f64::NAN;
    check(&sky, &bad, ship, 0.2);
    let mut bad = cam;
    bad.target = bad.position;
    check(&sky, &bad, ship, 0.2);
    let mut short = Framebuffer::new(64, 36, SPACE_COLOR).unwrap();
    short.pixels.pop();
    check(&short, &cam, ship, 0.2);
    let mut overflow = Framebuffer::new(64, 36, SPACE_COLOR).unwrap();
    overflow.width = usize::MAX;
    check(&overflow, &cam, ship, 0.2);
    let mut empty = Framebuffer::new(64, 36, SPACE_COLOR).unwrap();
    (empty.width, empty.height) = (0, 36);
    check(&empty, &cam, ship, 0.2);
}

#[test]
fn born_green_zero_oxygen_combined_stays_black() {
    for name in ["normal", "near3", "sky"] {
        let (world, frame) = view(name, 0.0);
        let (_, new) = both(&world, &frame, 0.0);
        assert!(
            new.pixels != frame.pixels,
            "{name}: the eyes exist before the post at 0%"
        );
        assert!(
            combined(&world, &frame, &new.pixels)
                .iter()
                .all(|&p| p == 0),
            "{name}: 0% combined must stay total black (eyes go UNDER the approved post)"
        );
    }
}
