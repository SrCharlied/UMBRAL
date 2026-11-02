//! Supervisor tracer: density at the user-specified 25% anchor, before art implementation.
#[path = "../src/visual/watching_stars.rs"]
mod watching_stars;

#[test]
fn at_twenty_five_percent_nearly_all_existing_stars_are_observers() {
    let weights = watching_stars::observer_weights(0.25).unwrap();
    assert_eq!(
        weights.len(),
        700,
        "use existing distant star identities, not a new random sky"
    );
    let observing = weights.iter().filter(|&&w| w >= 0.5).count();
    assert!(
        observing >= 630,
        "at25% nearly all stars must observe, not the unchanged ordinary sky: {observing}/700"
    );
}

// ---------------------------------------------------------------------------------------------
// Pruebas del worker (Bronya). El prefijo de arriba es el tracer del supervisor, intacto.
// ---------------------------------------------------------------------------------------------
#[path = "fixtures/reference_stars.rs"]
mod reference_stars;

use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::framebuffer::Framebuffer,
    scene::{
        camera::Camera,
        world::{BASE_FOV, SPACE_COLOR, World},
    },
};

fn camera(width: usize, height: usize, position: Vec3, target: Vec3, up: Vec3) -> Camera {
    let mut c = Camera::new(BASE_FOV, width as f64 / height as f64, 0.3, 6000.0).unwrap();
    (c.position, c.target, c.up) = (position, target, up);
    c
}

fn world_views(width: usize, height: usize) -> Vec<(&'static str, World)> {
    let model = || load_model(&ship_model_path()).unwrap();
    let aspect = width as f64 / height as f64;
    let normal = World::new(model(), aspect).unwrap();
    let mut near = World::new(model(), aspect).unwrap();
    near.place_near(3, 25.0).unwrap();
    vec![("normal", normal), ("near3", near)]
}

/// Cielo del catalogo con el mismo orden y z-test de `draw_stars`.
fn catalog_sky(width: usize, height: usize, cam: &Camera) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    let vp = watching_stars::sky_view_projection(cam);
    for star in watching_stars::star_catalog() {
        if let Some((x, y)) = watching_stars::star_pixel(&vp, star.direction, width, height) {
            fb.write(x, y, 1.0, star.color);
        }
    }
    fb
}

fn fixture_sky(width: usize, height: usize, cam: &Camera) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    reference_stars::reference_stars(&mut fb, cam);
    fb
}

#[test]
fn catalog_reproduces_the_published_sky_bit_exactly() {
    let catalog = watching_stars::star_catalog();
    assert_eq!(catalog.len(), watching_stars::STAR_COUNT);
    assert!(catalog.iter().enumerate().all(|(i, s)| s.id == i));
    let mut cams = vec![
        camera(
            1280,
            720,
            Vec3::new(0.0, 46.0, 722.0),
            Vec3::new(0.0, 40.0, 670.0),
            Vec3::Y,
        ),
        camera(
            1280,
            720,
            Vec3::new(3.0, -2.0, 1.0),
            Vec3::new(-4.0, 5.0, 2.0),
            Vec3::new(0.3, 1.0, 0.1),
        ),
        camera(1280, 720, Vec3::ZERO, Vec3::Y, Vec3::Y),
    ];
    for (_, world) in world_views(1280, 720) {
        cams.push(world.camera);
    }
    for (w, h) in [(1280, 720), (320, 180), (7, 3), (1, 1), (1, 9), (9, 1)] {
        for cam in &cams {
            let expected = fixture_sky(w, h, cam);
            assert!(catalog_sky(w, h, cam).pixels == expected.pixels, "{w}x{h}");
        }
    }
    // El cielo de referencia tiene estrellas encendidas: la comparacion no es trivial.
    let lit = fixture_sky(1280, 720, &cams[0])
        .pixels
        .iter()
        .filter(|&&p| p != SPACE_COLOR)
        .count();
    assert!(lit > 50, "{lit}");
}

#[test]
fn translating_the_camera_does_not_move_the_infinite_sky() {
    let base = camera(
        1280,
        720,
        Vec3::new(0.0, 46.0, 722.0),
        Vec3::new(0.0, 40.0, 670.0),
        Vec3::Y,
    );
    let offset = Vec3::new(-2500.0, 300.0, 4000.0);
    let moved = camera(
        1280,
        720,
        base.position + offset,
        base.target + offset,
        Vec3::Y,
    );
    let (a, b) = (
        watching_stars::sky_view_projection(&base),
        watching_stars::sky_view_projection(&moved),
    );
    let mut on_screen = 0;
    for star in watching_stars::star_catalog() {
        let p = watching_stars::star_pixel(&a, star.direction, 1280, 720);
        assert_eq!(p, watching_stars::star_pixel(&b, star.direction, 1280, 720));
        on_screen += usize::from(p.is_some());
    }
    assert!(on_screen > 50, "{on_screen}");
    assert!(fixture_sky(1280, 720, &base).pixels == fixture_sky(1280, 720, &moved).pixels);
}

#[test]
fn stars_behind_the_camera_have_no_pixel() {
    let cam = camera(64, 64, Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), Vec3::Y);
    let vp = watching_stars::sky_view_projection(&cam);
    assert_eq!(
        watching_stars::star_pixel(&vp, Vec3::new(0.0, 0.0, 1.0), 64, 64),
        None
    );
    assert_eq!(
        watching_stars::star_pixel(&vp, Vec3::new(0.0, 0.0, -1.0), 64, 64),
        Some((32, 32))
    );
}

const LEGIBLE: f64 = 0.5;

fn counts(oxygen01: f64) -> (usize, usize) {
    let w = watching_stars::observer_weights(oxygen01).unwrap();
    (
        w.iter().filter(|&&v| v > 0.0).count(),
        w.iter().filter(|&&v| v >= LEGIBLE).count(),
    )
}

#[test]
fn high_oxygen_has_no_observer_at_all() {
    for o in [1.0, 0.9, 0.7, 0.66, 0.65] {
        let w = watching_stars::observer_weights(o).unwrap();
        assert!(w.iter().all(|&v| v == 0.0), "{o}");
        assert_eq!(watching_stars::observer_density(o).unwrap(), 0.0);
    }
}

#[test]
fn sixty_percent_is_an_extremely_sparse_start_on_the_whole_sphere() {
    let (drawn, legible) = counts(0.6);
    // Calibrado tras medir la oclusion de la nave en el arranque normal (ver informe): <= 5 % con
    // peso y <= 3 % legibles en toda la esfera.
    assert!(
        (4..=35).contains(&drawn),
        "w>0 at 60%: {drawn}/700 (<= 5 %)"
    );
    assert!(
        (1..=21).contains(&legible),
        "legible at 60%: {legible}/700 (<= 3 %)"
    );
    assert!(watching_stars::observer_density(0.6).unwrap() < 0.03);
}

#[test]
fn fifty_percent_is_clearly_more_but_not_nearly_all() {
    let (_, at60) = counts(0.6);
    let (_, at50) = counts(0.5);
    assert!(at50 >= 4 * at60.max(1), "{at60} -> {at50}");
    assert!(at50 <= 245, "not nearly all at 50%: {at50}/700 (<= 35 %)");
    let density = watching_stars::observer_density(0.5).unwrap();
    assert!((0.1..0.35).contains(&density), "{density}");
}

#[test]
fn nearly_all_from_twenty_five_percent_down_and_all_at_zero() {
    assert!(counts(0.25).1 >= 630);
    assert!(counts(0.1).1 >= 690);
    let w = watching_stars::observer_weights(0.0).unwrap();
    assert!(w.iter().all(|&v| v == 1.0));
}

#[test]
fn weights_follow_a_stable_ranking_nested_and_monotone() {
    let ranking = watching_stars::observer_ranking();
    let mut sorted = ranking.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        (0..watching_stars::STAR_COUNT).collect::<Vec<_>>(),
        "permutation of the IDs"
    );
    assert_eq!(
        ranking,
        watching_stars::observer_ranking(),
        "fixed, not rerolled"
    );
    let mut previous = vec![0.0; watching_stars::STAR_COUNT];
    for step in 0..=400 {
        let o = 1.0 - step as f64 / 400.0;
        let w = watching_stars::observer_weights(o).unwrap();
        assert!(w.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
        // Al bajar el O2 ninguna estrella pierde peso (acumulativo y anidado).
        assert!(w.iter().zip(&previous).all(|(a, b)| a >= b), "{o}");
        // En el orden del ranking los pesos nunca suben: las primeras siempre van delante.
        assert!(ranking.windows(2).all(|p| w[p[0]] >= w[p[1]]), "{o}");
        previous = w;
    }
    let w = watching_stars::observer_weights(0.6).unwrap();
    assert!(w[ranking[0]] >= LEGIBLE && w[ranking[699]] == 0.0);
}

#[test]
fn curve_is_continuous_and_recovers_the_same_identities() {
    let eps = 1e-4;
    let mut o: f64 = 0.0;
    while o + eps <= 1.0 {
        let (a, b) = (
            watching_stars::observer_weights(o).unwrap(),
            watching_stars::observer_weights(o + eps).unwrap(),
        );
        let jump = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max);
        assert!(jump <= 0.01, "per-star jump {jump} at {o}");
        o += 0.0137;
    }
    // Bajar y volver a subir: los mismos IDs y pesos, no un sorteo nuevo.
    let down = watching_stars::observer_weights(0.45).unwrap();
    let _ = watching_stars::observer_weights(0.05).unwrap();
    assert_eq!(watching_stars::observer_weights(0.45).unwrap(), down);
}

#[test]
fn invalid_oxygen_is_rejected() {
    for o in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001, 1.001] {
        assert!(watching_stars::observer_weights(o).is_err(), "{o}");
        assert!(watching_stars::observer_density(o).is_err(), "{o}");
    }
}

#[test]
fn first_observers_are_spread_over_the_sphere() {
    let catalog = watching_stars::star_catalog();
    let ranking = watching_stars::observer_ranking();
    let first: Vec<Vec3> = ranking[..16]
        .iter()
        .map(|&i| catalog[i].direction)
        .collect();
    let mut min_deg = f64::MAX;
    for (a, u) in first.iter().enumerate() {
        for v in &first[a + 1..] {
            min_deg = min_deg.min(u.dot(*v).clamp(-1.0, 1.0).acos().to_degrees());
        }
    }
    assert!(
        min_deg >= 35.0,
        "first 16 observers min separation {min_deg:.1} deg"
    );
}

#[test]
fn at_sixty_percent_any_view_sees_only_a_few_observers() {
    let catalog = watching_stars::star_catalog();
    let w = watching_stars::observer_weights(0.6).unwrap();
    let (mut drawn, mut legible, mut worst) = (0, 0, 0);
    let views = 300;
    for k in 0..views {
        // Direcciones de vista deterministas (espiral de Fibonacci), sin traslacion ni oclusion.
        let z = 1.0 - 2.0 * (k as f64 + 0.5) / views as f64;
        let a = k as f64 * 2.399_963_229_728_653;
        let r = (1.0 - z * z).sqrt();
        let forward = Vec3::new(r * a.cos(), z, r * a.sin());
        let up = if z.abs() > 0.99 { Vec3::X } else { Vec3::Y };
        let cam = camera(1280, 720, Vec3::ZERO, forward, up);
        let vp = watching_stars::sky_view_projection(&cam);
        let seen: Vec<f64> = catalog
            .iter()
            .filter(|s| watching_stars::star_pixel(&vp, s.direction, 1280, 720).is_some())
            .map(|s| w[s.id])
            .filter(|&v| v > 0.0)
            .collect();
        drawn += seen.len();
        legible += seen.iter().filter(|&&v| v >= LEGIBLE).count();
        worst = worst.max(seen.len());
    }
    let (drawn, legible) = (drawn as f64 / views as f64, legible as f64 / views as f64);
    // Perceptual: "ver una o 2" son los legibles; los de peso < 0.5 estan abriendo (fundido).
    assert!(
        (1.0..=2.5).contains(&legible),
        "mean legible per view at 60%: {legible:.2}"
    );
    assert!(
        drawn <= 3.5 && worst <= 5,
        "drawn per view at 60%: mean {drawn:.2}, worst {worst}"
    );
}

// --- Render ----------------------------------------------------------------------------------

/// Caja maxima de un ojo alrededor del pixel de su estrella: 9x5.
const EYE_BOX: (isize, isize) = (4, 2);

fn reference_camera() -> Camera {
    camera(
        1280,
        720,
        Vec3::new(0.0, 46.0, 722.0),
        Vec3::new(0.0, 40.0, 670.0),
        Vec3::Y,
    )
}

/// La nave de la camara de persecucion del arranque normal.
const SHIP: Vec3 = Vec3::new(0.0, 40.0, 700.0);

fn changed(a: &[u32], b: &[u32]) -> Vec<usize> {
    (0..a.len()).filter(|&i| a[i] != b[i]).collect()
}

fn in_box(i: usize, width: usize, (x, y): (isize, isize)) -> bool {
    let (px, py) = ((i % width) as isize, (i / width) as isize);
    (px - x).abs() <= EYE_BOX.0 && (py - y).abs() <= EYE_BOX.1
}

fn luma(p: u32) -> f64 {
    let c = |s: u32| f64::from((p >> s) & 0xff);
    0.299 * c(16) + 0.587 * c(8) + 0.114 * c(0)
}

#[test]
fn eyes_have_a_small_but_more_than_one_pixel_footprint() {
    let cam = reference_camera();
    let sky = fixture_sky(1280, 720, &cam);
    let out = watching_stars::render_watching_stars(&sky, &cam, SHIP, 0.25).unwrap();
    let eyes = &out.report.eyes;
    assert!(
        eyes.len() >= 60,
        "nearly all visible stars observe at 25%: {}",
        eyes.len()
    );
    let diff = changed(&sky.pixels, &out.pixels);
    // Nada fuera de las cajas de 9x5 de los ojos: distantes, no caras enormes.
    assert!(
        diff.iter()
            .all(|&i| eyes.iter().any(|e| in_box(i, 1280, e.pixel)))
    );
    let open: Vec<_> = eyes.iter().filter(|e| e.weight == 1.0).collect();
    assert!(!open.is_empty());
    for e in open {
        let own = diff.iter().filter(|&&i| in_box(i, 1280, e.pixel)).count();
        assert!(
            own >= 15,
            "eye {} changes only {own} px: a brighter dot is not an eye",
            e.id
        );
        assert!(
            e.pixels_changed >= 15 && e.pixels_changed <= 45,
            "{}",
            e.pixels_changed
        );
    }
}

/// Centroide de oscuridad (pupila e iris) en el nucleo de 5x3 del ojo, relativo a su estrella. El
/// nucleo queda dentro del parpado con el ojo abierto: el contorno (simetrico) no lo diluye.
fn pupil_centroid(sky: &Framebuffer, out: &[u32], e: &watching_stars::ObserverEye) -> [f64; 2] {
    let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
    for dy in -1..=1 {
        for dx in -2..=2 {
            let (x, y) = (e.pixel.0 + dx, e.pixel.1 + dy);
            if x < 0 || y < 0 || x as usize >= sky.width || y as usize >= sky.height {
                continue;
            }
            let i = y as usize * sky.width + x as usize;
            if out[i] != sky.pixels[i] {
                // Oscuro dentro del ojo: lo que la pupila apaga respecto de la esclerotica.
                let dark = (255.0 - luma(out[i])).max(0.0);
                (sx, sy, sw) = (sx + dark * dx as f64, sy + dark * dy as f64, sw + dark);
            }
        }
    }
    [sx / sw, sy / sw]
}

#[test]
fn pupils_look_at_the_projected_off_center_ship() {
    let cam = reference_camera();
    let sky = fixture_sky(1280, 720, &cam);
    let vp = cam.view_projection();
    // Nave descentrada arriba a la izquierda del cuadro (no el centro ni el objetivo de camara).
    let forward = (cam.target - cam.position).normalized();
    let right = forward.cross(Vec3::Y).normalized();
    let up = right.cross(forward);
    let ship = cam.position + (forward - right * 0.62 + up * 0.30).normalized() * 40.0;
    let c = vp.transform(ship.extend(1.0));
    let focus = [
        (c.x / c.w * 0.5 + 0.5) * 1280.0,
        (0.5 - c.y / c.w * 0.5) * 720.0,
    ];
    assert!(focus[0] < 300.0 && focus[1] < 260.0, "{focus:?}");
    let out = watching_stars::render_watching_stars(&sky, &cam, ship, 0.0).unwrap();
    let got = out
        .report
        .ship_focus_px
        .expect("ship in front of the camera");
    assert!((got[0] - focus[0]).abs() < 1e-9 && (got[1] - focus[1]).abs() < 1e-9);
    let mut checked = 0;
    for e in out.report.eyes.iter().filter(|e| e.weight == 1.0) {
        let to_ship = [
            focus[0] - e.pixel.0 as f64 - 0.5,
            focus[1] - e.pixel.1 as f64 - 0.5,
        ];
        let to_center = [
            640.0 - e.pixel.0 as f64 - 0.5,
            360.0 - e.pixel.1 as f64 - 0.5,
        ];
        let len = to_ship[0].hypot(to_ship[1]);
        // Solo los ojos donde mirar a la nave y mirar al centro dan lados opuestos en x.
        if len < 40.0 || to_ship[0].signum() == to_center[0].signum() || to_ship[0].abs() < 20.0 {
            continue;
        }
        let o = e.pupil_offset;
        let cos = (o[0] * to_ship[0] + o[1] * to_ship[1]) / (o[0].hypot(o[1]) * len);
        assert!(
            cos > 0.999,
            "eye {} pupil {o:?} vs ship direction {to_ship:?}",
            e.id
        );
        // En pixeles: la oscuridad del nucleo se corre hacia la nave.
        let p = pupil_centroid(&sky, &out.pixels, e);
        let along = (p[0] * to_ship[0] + p[1] * to_ship[1]) / len;
        // Con la pupila centrada es 0 por simetria; en vertical el nucleo 5x3 mide ~0.1 px.
        assert!(
            along > 0.08,
            "eye {} dark centroid {p:?} vs {to_ship:?}",
            e.id
        );
        if to_ship[0].abs() > 0.5 * len {
            assert!(
                p[0].signum() == to_ship[0].signum(),
                "eye {} dark centroid {p:?}",
                e.id
            );
        }
        checked += 1;
    }
    assert!(checked >= 3, "{checked}");
}

#[test]
fn ship_without_projection_keeps_the_pupils_centered() {
    let cam = reference_camera();
    let sky = fixture_sky(1280, 720, &cam);
    let behind = cam.position + (cam.position - cam.target).normalized() * 30.0;
    let out = watching_stars::render_watching_stars(&sky, &cam, behind, 0.0).unwrap();
    assert_eq!(out.report.ship_focus_px, None);
    assert!(!out.report.eyes.is_empty());
    assert!(out.report.eyes.iter().all(|e| e.pupil_offset == [0.0, 0.0]));
}

#[test]
fn geometry_in_front_hides_eyes_completely_and_clips_them_partially() {
    let cam = reference_camera();
    let mut fb = fixture_sky(1280, 720, &cam);
    let free = watching_stars::render_watching_stars(&fb, &cam, SHIP, 0.0).unwrap();
    let target = free.report.eyes.iter().find(|e| {
        e.weight == 1.0 && e.pixel.0 > 20 && e.pixel.1 > 20 && e.pixel.0 < 600 && e.pixel.1 < 700
    });
    let target = target.expect("an open eye on the left half").clone();
    // "Planeta" opaco en toda la mitad derecha y una franja que tapa la mitad derecha del ojo
    // elegido sin su centro.
    for y in 0..720 {
        for x in 640..1280 {
            fb.write(x, y, 0.4, 0x203040);
        }
    }
    for dy in -EYE_BOX.1..=EYE_BOX.1 {
        for dx in 1..=EYE_BOX.0 {
            fb.write(target.pixel.0 + dx, target.pixel.1 + dy, 0.9999, 0x405060);
        }
    }
    let out = watching_stars::render_watching_stars(&fb, &cam, SHIP, 0.0).unwrap();
    let diff = changed(&fb.pixels, &out.pixels);
    assert!(
        diff.iter().all(|&i| i % 1280 < 640),
        "nothing painted over the opaque body"
    );
    let eye = out
        .report
        .eyes
        .iter()
        .find(|e| e.id == target.id)
        .expect("center still visible");
    let own: Vec<_> = diff
        .iter()
        .filter(|&&i| in_box(i, 1280, eye.pixel))
        .collect();
    assert!(!own.is_empty() && own.iter().all(|&&i| ((i % 1280) as isize) <= eye.pixel.0));
    assert!(eye.pixels_changed < target.pixels_changed);
    // Los que quedaron detras del cuerpo: registrados como ocluidos y sin ojo.
    let hidden: Vec<_> = free
        .report
        .eyes
        .iter()
        .filter(|e| e.pixel.0 >= 640)
        .map(|e| e.id)
        .collect();
    assert!(!hidden.is_empty());
    assert!(hidden.iter().all(|id| out.report.occluded_ids.contains(id)));
    assert!(out.report.eyes.iter().all(|e| !hidden.contains(&e.id)));
}

#[test]
fn source_pixels_and_depth_stay_intact() {
    let cam = reference_camera();
    let mut fb = fixture_sky(1280, 720, &cam);
    for y in 300..420 {
        for x in 500..760 {
            fb.write(x, y, 0.5, 0x406080);
        }
    }
    let before = fb.pixels.clone();
    let probe = watching_stars::SKY_DEPTH_PROBE;
    let vp = watching_stars::sky_view_projection(&cam);
    let mut probes: Vec<(isize, isize, f64)> = Vec::new();
    for (x, y) in [(0, 0), (639, 359), (500, 300), (1279, 719), (5, 700)] {
        for d in [0.3, 0.5, 0.75, probe, 1.0, 2.0] {
            probes.push((x, y, d));
        }
    }
    for s in watching_stars::star_catalog() {
        if let Some((x, y)) = watching_stars::star_pixel(&vp, s.direction, 1280, 720) {
            probes.push((x, y, probe));
            probes.push((x, y, 1.0));
        }
    }
    let depth = |fb: &Framebuffer| {
        probes
            .iter()
            .map(|&(x, y, d)| fb.depth_passes(x, y, d))
            .collect::<Vec<_>>()
    };
    let depth_before = depth(&fb);
    // La estrella existente (profundidad 1) pasa la sonda; la geometria (0.5) no.
    assert!(depth_before.iter().any(|&b| b) && depth_before.iter().any(|&b| !b));
    let out = watching_stars::render_watching_stars(&fb, &cam, SHIP, 0.1).unwrap();
    assert!(out.pixels != before, "eyes drawn at 10%");
    assert!(fb.pixels == before, "source pixels intact");
    assert_eq!(depth(&fb), depth_before);
    assert_eq!(probe, 1.0f64.next_down());
}

#[test]
fn high_oxygen_frames_are_exact() {
    for (_, world) in world_views(320, 180) {
        let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        world.render(&mut fb);
        for o in [1.0, 0.7, 0.65] {
            let out =
                watching_stars::render_watching_stars(&fb, &world.camera, world.ship.position, o)
                    .unwrap();
            assert!(out.pixels == fb.pixels, "exact frame at {o}");
            assert!(out.report.eyes.is_empty() && out.report.active_ids.is_empty());
        }
    }
}

#[test]
fn invalid_inputs_are_rejected() {
    let cam = reference_camera();
    let sky = fixture_sky(64, 36, &cam);
    let call = |fb: &Framebuffer, cam: &Camera, ship: Vec3, o: f64| {
        watching_stars::render_watching_stars(fb, cam, ship, o)
    };
    for o in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(call(&sky, &cam, SHIP, o).is_err(), "{o}");
    }
    for ship in [
        Vec3::new(f64::NAN, 0.0, 0.0),
        Vec3::new(0.0, f64::INFINITY, 0.0),
    ] {
        assert!(call(&sky, &cam, ship, 0.2).is_err());
    }
    let mut bad = cam;
    bad.position.x = f64::NAN;
    assert!(call(&sky, &bad, SHIP, 0.2).is_err());
    let mut bad = cam;
    bad.target = bad.position;
    assert!(call(&sky, &bad, SHIP, 0.2).is_err());
    let mut short = fixture_sky(64, 36, &cam);
    short.pixels.pop();
    assert!(call(&short, &cam, SHIP, 0.2).is_err());
    let mut overflow = fixture_sky(64, 36, &cam);
    overflow.width = usize::MAX;
    assert!(call(&overflow, &cam, SHIP, 0.2).is_err());
    let mut empty = fixture_sky(64, 36, &cam);
    (empty.width, empty.height) = (0, 36);
    assert!(call(&empty, &cam, SHIP, 0.2).is_err());
    let mut swapped = fixture_sky(64, 36, &cam);
    swapped.width = 63;
    assert!(call(&swapped, &cam, SHIP, 0.2).is_err());
}

#[test]
fn tiny_and_non_square_frames_clip_the_eye() {
    let catalog = watching_stars::star_catalog();
    let ranking = watching_stars::observer_ranking();
    let star = catalog[ranking[0]];
    for (w, h) in [(1, 1), (1, 7), (7, 1), (2, 3), (37, 5), (5, 37)] {
        let up = if star.direction.y.abs() > 0.99 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let cam = camera(w, h, Vec3::ZERO, star.direction, up);
        let sky = fixture_sky(w, h, &cam);
        let ship = star.direction * 30.0 + Vec3::new(0.0, -3.0, 0.0);
        let out = watching_stars::render_watching_stars(&sky, &cam, ship, 0.0).unwrap();
        assert_eq!(out.pixels.len(), w * h);
        let eye = out.report.eyes.iter().find(|e| e.id == star.id);
        let eye = eye.expect("first observer at the center");
        assert!(
            eye.pixels_changed >= 1 && eye.pixels_changed <= w * h,
            "{w}x{h}"
        );
    }
}

#[test]
fn report_separates_policy_from_what_was_drawn() {
    let cam = reference_camera();
    let mut fb = fixture_sky(1280, 720, &cam);
    for y in 0..720 {
        for x in 0..300 {
            fb.write(x, y, 0.5, 0x101010);
        }
    }
    let ranking = watching_stars::observer_ranking();
    for o in [0.6, 0.5, 0.25] {
        let out = watching_stars::render_watching_stars(&fb, &cam, SHIP, o).unwrap();
        let r = &out.report;
        let w = watching_stars::observer_weights(o).unwrap();
        let active: Vec<usize> = (0..700).filter(|&i| w[i] > 0.0).collect();
        assert_eq!(r.active_ids, active);
        assert_eq!(r.density, watching_stars::observer_density(o).unwrap());
        assert!(r.projected_ids.iter().all(|i| r.active_ids.contains(i)));
        assert!(r.occluded_ids.iter().all(|i| r.projected_ids.contains(i)));
        assert!(
            r.shadowed_ids
                .iter()
                .all(|i| r.active_ids.contains(i) && !r.projected_ids.contains(i))
        );
        let eyes: Vec<usize> = r.eyes.iter().map(|e| e.id).collect();
        let visible: Vec<usize> = r
            .projected_ids
            .iter()
            .copied()
            .filter(|i| !r.occluded_ids.contains(i))
            .collect();
        assert_eq!(eyes, visible, "{o}");
        assert!(!eyes.is_empty(), "{o}");
        if o == 0.25 {
            assert!(!r.occluded_ids.is_empty());
        }
        assert!(
            r.eyes
                .iter()
                .all(|e| e.weight == w[e.id] && e.pixels_changed > 0)
        );
        assert!(r.eyes.iter().all(|e| ranking[e.rank] == e.id));
    }
}

#[test]
fn eyes_are_deterministic_and_fixed_on_the_infinite_sky() {
    let cam = reference_camera();
    // Potencias de 2: `target - position` es bit-identico tras trasladar (la rotacion tambien).
    let offset = Vec3::new(1024.0, -128.0, -2048.0);
    let moved = camera(
        1280,
        720,
        cam.position + offset,
        cam.target + offset,
        Vec3::Y,
    );
    let (a, b) = (fixture_sky(1280, 720, &cam), fixture_sky(1280, 720, &moved));
    let first = watching_stars::render_watching_stars(&a, &cam, SHIP, 0.4).unwrap();
    assert!(first == watching_stars::render_watching_stars(&a, &cam, SHIP, 0.4).unwrap());
    let second = watching_stars::render_watching_stars(&b, &moved, SHIP + offset, 0.4).unwrap();
    assert!(!first.report.eyes.is_empty());
    assert!(
        first.pixels == second.pixels,
        "same eye pixels when camera and ship translate together"
    );
    let (a, b) = (&first.report, &second.report);
    assert_eq!(
        (&a.active_ids, &a.projected_ids, &a.occluded_ids),
        (&b.active_ids, &b.projected_ids, &b.occluded_ids)
    );
    assert_eq!(a.eyes.len(), b.eyes.len());
    // La nave se proyecta con la traslacion: solo redondeo en el foco y la pupila.
    let (fa, fb) = (a.ship_focus_px.unwrap(), b.ship_focus_px.unwrap());
    assert!((fa[0] - fb[0]).abs() < 1e-6 && (fa[1] - fb[1]).abs() < 1e-6);
    for (x, y) in a.eyes.iter().zip(&b.eyes) {
        assert_eq!(
            (x.id, x.rank, x.pixel, x.weight, x.pixels_changed),
            (y.id, y.rank, y.pixel, y.weight, y.pixels_changed)
        );
        assert!((x.pupil_offset[0] - y.pupil_offset[0]).abs() < 1e-6);
        assert!((x.pupil_offset[1] - y.pupil_offset[1]).abs() < 1e-6);
    }
}

/// (vista, ojos dibujados, legibles) en las dos vistas reales del World a 1280x720.
fn world_eyes(o: f64) -> Vec<(&'static str, usize, usize)> {
    world_views(1280, 720)
        .into_iter()
        .map(|(name, mut world)| {
            world.debug_oxygen01 = o;
            let mut fb = Framebuffer::new(1280, 720, SPACE_COLOR).unwrap();
            world.render(&mut fb);
            let out =
                watching_stars::render_watching_stars(&fb, &world.camera, world.ship.position, o)
                    .unwrap();
            let legible = out
                .report
                .eyes
                .iter()
                .filter(|e| e.weight >= LEGIBLE)
                .count();
            (name, out.report.eyes.len(), legible)
        })
        .collect()
}

#[test]
fn reference_views_show_one_or_two_at_sixty_and_nearly_all_at_twenty_five() {
    let at60 = world_eyes(0.6);
    let (_, normal, normal_legible) = at60[0];
    assert!(
        (1..=2).contains(&normal) && normal_legible >= 1,
        "reference view at 60%: {at60:?}"
    );
    assert!(at60[1].1 <= 4, "{at60:?}");
    let at50 = world_eyes(0.5);
    let at25 = world_eyes(0.25);
    for k in 0..2 {
        assert!(at50[k].2 >= 3 * at60[k].2.max(1), "{at60:?} {at50:?}");
        assert!(at25[k].2 >= 2 * at50[k].2, "{at50:?} {at25:?}");
    }
}

#[test]
fn only_the_star_that_draw_stars_shows_in_a_pixel_gets_an_eye() {
    // En 1x1 todas las estrellas de la vista caen en el mismo pixel: draw_stars deja la primera.
    let cam = camera(1, 1, Vec3::ZERO, Vec3::new(0.3, 0.1, -1.0), Vec3::Y);
    let sky = fixture_sky(1, 1, &cam);
    let vp = watching_stars::sky_view_projection(&cam);
    let inside: Vec<usize> = watching_stars::star_catalog()
        .iter()
        .filter(|s| watching_stars::star_pixel(&vp, s.direction, 1, 1).is_some())
        .map(|s| s.id)
        .collect();
    assert!(inside.len() >= 2, "{inside:?}");
    let out = watching_stars::render_watching_stars(&sky, &cam, SHIP, 0.0).unwrap();
    let r = &out.report;
    assert_eq!(r.projected_ids, vec![inside[0]]);
    assert_eq!(r.shadowed_ids, inside[1..].to_vec());
    assert_eq!(
        r.eyes.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![inside[0]]
    );
    assert_eq!(r.oxygen01, 0.0);
}
