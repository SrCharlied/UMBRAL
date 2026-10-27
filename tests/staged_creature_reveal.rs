//! Revelacion por etapas (primero el ojo, despues el cuerpo): ajustes, progreso unico,
//! particion por ROI, compositor generalizado, integracion con `World` y linea de comandos.
//! Fixtures sinteticos creados aqui; no necesita el asset real ni abre ventanas.

use umbral::visual::{creature_reveal::step_reveal, staged_creature_reveal::*};

// --- Ajustes y progreso ---

fn defaults() -> StagedRevealSettings {
    StagedRevealSettings::default()
}

#[test]
fn staged_settings_default_to_the_candidate_curves_and_reject_hostile_values() {
    let s = defaults();
    assert_eq!(
        (
            s.start_oxygen(),
            s.eye_full_oxygen(),
            s.hint_oxygen(),
            s.hint_alpha(),
            s.full_oxygen(),
            s.rise_seconds(),
            s.fall_seconds()
        ),
        (0.50, 0.30, 0.25, 0.02, 0.05, 2.0, 0.7)
    );
    assert_eq!(
        StagedRevealSettings::new(0.50, 0.30, 0.25, 0.02, 0.05, 2.0, 0.7),
        Ok(s)
    );
    let (nan, inf) = (f64::NAN, f64::INFINITY);
    let invalid = [
        (nan, 0.3, 0.25, 0.02, 0.05, 2.0, 0.7),
        (0.5, nan, 0.25, 0.02, 0.05, 2.0, 0.7),
        (0.5, 0.3, nan, 0.02, 0.05, 2.0, 0.7),
        (0.5, 0.3, 0.25, nan, 0.05, 2.0, 0.7),
        (0.5, 0.3, 0.25, 0.02, nan, 2.0, 0.7),
        (inf, 0.3, 0.25, 0.02, 0.05, 2.0, 0.7),
        (1.01, 0.3, 0.25, 0.02, 0.05, 2.0, 0.7),
        (0.5, 0.3, 0.25, 0.02, -0.01, 2.0, 0.7),
        // Orden estricto: full < hint < eye_full < start.
        (0.3, 0.3, 0.25, 0.02, 0.05, 2.0, 0.7),
        (0.5, 0.25, 0.25, 0.02, 0.05, 2.0, 0.7),
        (0.5, 0.3, 0.05, 0.02, 0.05, 2.0, 0.7),
        (0.5, 0.3, 0.4, 0.02, 0.05, 2.0, 0.7),
        // Alfa del indicio en [0, 1).
        (0.5, 0.3, 0.25, -0.01, 0.05, 2.0, 0.7),
        (0.5, 0.3, 0.25, 1.0, 0.05, 2.0, 0.7),
        // Tiempos como V1.
        (0.5, 0.3, 0.25, 0.02, 0.05, 0.0, 0.7),
        (0.5, 0.3, 0.25, 0.02, 0.05, 2.0, inf),
    ];
    for (a, b, c, d, e, f, g) in invalid {
        assert!(
            StagedRevealSettings::new(a, b, c, d, e, f, g).is_err(),
            "aceptado: {a} {b} {c} {d} {e} {f} {g}"
        );
    }
    assert!(StagedRevealSettings::new(1.0, 0.5, 0.4, 0.0, 0.0, 0.01, 60.0).is_ok());
}

#[test]
fn staged_settings_reject_collapsed_derived_breakpoints() {
    let below_half = f64::from_bits(0.5_f64.to_bits() - 1);
    let cases = [
        ("eye_equals_hint", 1.0, 0.5, below_half, 0.0),
        ("hint_equals_one", 1.0, 0.5, 1e-300, 0.0),
        ("both_equal_one", 1.0, 1e-300, 5e-301, 0.0),
    ];
    let mut accepted = Vec::new();
    for (name, start, eye, hint, full) in cases {
        match StagedRevealSettings::new(start, eye, hint, 0.02, full, 2.0, 0.7) {
            Ok(settings) => accepted.push((name, settings)),
            Err(error) => assert!(error.contains("umbrales"), "{name}: {error}"),
        }
    }
    assert!(
        accepted.is_empty(),
        "expected Err, accepted Ok: {accepted:?}"
    );
}

#[test]
fn staged_settings_keep_representable_narrow_intervals_and_finite_alphas() {
    let half_bits = 0.5_f64.to_bits();
    let cases = [
        (0.50, 0.30, 0.25, 0.05),
        (1.0, f64::from_bits(1.0_f64.to_bits() - 1), 0.5, 0.0),
        (1.0, 0.5, f64::EPSILON / 2.0, 0.0),
        (1.0, 0.5, f64::from_bits(half_bits - 2), 0.0),
        (
            0.5,
            f64::from_bits(half_bits - 1),
            f64::from_bits(half_bits - 2),
            f64::from_bits(half_bits - 3),
        ),
        (
            f64::from_bits(4),
            f64::from_bits(3),
            f64::from_bits(2),
            f64::from_bits(1),
        ),
    ];
    for (start, eye, hint, full) in cases {
        let s = StagedRevealSettings::new(start, eye, hint, 0.02, full, 2.0, 0.7)
            .expect("umbrales derivados distintos y representables");
        let eye_point = staged_target(eye, &s);
        let hint_point = staged_target(hint, &s);
        assert!(0.0 < eye_point && eye_point < hint_point && hint_point < 1.0);
        assert_eq!(staged_alphas(eye_point, &s), (1.0, 0.0));
        assert_eq!(staged_alphas(hint_point, &s), (1.0, 0.02));
        for oxygen in [0.0, full, hint, eye, start, 1.0] {
            let p = staged_target(oxygen, &s);
            assert!(p.is_finite() && (0.0..=1.0).contains(&p));
            let (eye_alpha, body_alpha) = staged_alphas(p, &s);
            assert!(eye_alpha.is_finite() && (0.0..=1.0).contains(&eye_alpha));
            assert!(body_alpha.is_finite() && (0.0..=1.0).contains(&body_alpha));
        }
        for i in 0..=1000 {
            let (eye_alpha, body_alpha) = staged_alphas(i as f64 / 1000.0, &s);
            assert!(eye_alpha.is_finite() && (0.0..=1.0).contains(&eye_alpha));
            assert!(body_alpha.is_finite() && (0.0..=1.0).contains(&body_alpha));
            assert!(body_alpha == 0.0 || eye_alpha == 1.0);
        }
    }
}

#[test]
fn one_normalized_progress_gives_eye_then_hint_then_body_at_the_debug_steps() {
    let s = defaults();
    let at = |o2: f64| staged_alphas(staged_target(o2, &s), &s);
    assert_eq!(staged_target(1.0, &s), 0.0);
    assert_eq!(at(1.0), (0.0, 0.0));
    assert_eq!(at(0.50), (0.0, 0.0));
    let (eye40, body40) = at(0.40);
    assert!(
        eye40 > 0.0 && eye40 < 1.0 && body40 == 0.0,
        "{eye40} {body40}"
    );
    // 30 %: ojo completo, cuerpo todavia en 0.
    assert_eq!(at(0.30), (1.0, 0.0));
    // 25 %: ojo completo y contorno muy tenue (2 % de opacidad lineal).
    assert_eq!(at(0.25), (1.0, 0.02));
    let (eye20, body20) = at(0.20);
    assert!(eye20 == 1.0 && body20 > 0.02 && body20 < 1.0, "{body20}");
    // 5 % y menos: entera, progreso exactamente 1.
    assert_eq!(staged_target(0.05, &s), 1.0);
    assert_eq!(at(0.05), (1.0, 1.0));
    assert_eq!(at(0.0), (1.0, 1.0));
    // No finito: oculta.
    for o2 in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(staged_target(o2, &s), 0.0);
    }
    // Monotono en O2 y en [0, 1]; alfas monotonas y en [0, 1].
    let mut previous = (1.0, 1.0, 1.0);
    for i in 0..=1000 {
        let p = staged_target(i as f64 / 1000.0, &s);
        let (eye, body) = staged_alphas(p, &s);
        assert!(
            (0.0..=1.0).contains(&p) && (0.0..=1.0).contains(&eye) && (0.0..=1.0).contains(&body)
        );
        assert!(p <= previous.0 + 1e-15 && eye <= previous.1 + 1e-15 && body <= previous.2 + 1e-15);
        previous = (p, eye, body);
    }
}

#[test]
fn the_body_never_shows_with_a_partial_eye_for_any_progress_or_easing() {
    let s = defaults();
    for i in 0..=100_000 {
        let p = i as f64 / 100_000.0;
        let (eye, body) = staged_alphas(p, &s);
        assert!(body == 0.0 || eye == 1.0, "p {p}: ojo {eye} cuerpo {body}");
    }
    for p in [f64::NAN, -1.0, 2.0, f64::INFINITY] {
        let (eye, body) = staged_alphas(p, &s);
        assert!((0.0..=1.0).contains(&eye) && (0.0..=1.0).contains(&body));
        assert!(body == 0.0 || eye == 1.0);
    }
    // Aparecer y recuperar O2 con el estado real, cuadro a cuadro.
    for (from, to) in [
        (1.0, 0.05),
        (0.05, 1.0),
        (0.40, 0.20),
        (0.20, 0.40),
        (0.25, 1.0),
    ] {
        let mut reveal = StagedReveal::new(s, from);
        assert_eq!(
            reveal.shown(),
            staged_target(from, &s),
            "arranca en el objetivo"
        );
        for _ in 0..600 {
            reveal.update(to, 1.0 / 60.0);
            let (eye, body) = reveal.alphas();
            assert!(
                body == 0.0 || eye == 1.0,
                "{from}->{to}: {}",
                reveal.shown()
            );
        }
        assert_eq!(
            reveal.shown(),
            staged_target(to, &s),
            "{from}->{to} llega exacto"
        );
    }
}

#[test]
fn the_staged_progress_eases_with_real_time_like_v1() {
    let s = defaults();
    let mut reveal = StagedReveal::new(s, 1.0);
    reveal.update(0.05, 2.0);
    assert!((reveal.shown() - 0.95).abs() < 1e-12, "{}", reveal.shown());
    let mut back = StagedReveal::new(s, 0.05);
    back.update(1.0, 0.7);
    assert!((back.shown() - 0.05).abs() < 1e-12, "{}", back.shown());
    // Igual que `step_reveal` de V1 sobre el progreso normalizado: troceado y cuadros largos.
    let once = {
        let mut r = StagedReveal::new(s, 1.0);
        r.update(0.20, 0.25);
        r.shown()
    };
    let mut split = StagedReveal::new(s, 1.0);
    for _ in 0..25 {
        split.update(0.20, 0.01);
    }
    assert!((once - split.shown()).abs() < 1e-12);
    let target = staged_target(0.20, &s);
    let v1 = umbral::visual::creature_reveal::RevealSettings::default();
    assert!((once - step_reveal(0.0, target, 0.25, &v1)).abs() < 1e-12);
    // Snap y entradas defensivas.
    let mut r = StagedReveal::new(s, 1.0);
    r.update(0.25, f64::NAN);
    assert_eq!(r.shown(), 0.0);
    r.snap(0.25);
    assert_eq!(r.alphas(), (1.0, 0.02));
    assert_eq!(r.settings(), s);
}

// --- Particion por ROI (variante B: todas las caras de la ROI) ---

use umbral::{
    assets::obj::{Model, parse_obj},
    math::Vec3,
    visual::static_creature::CreaturePlacement,
};

/// Ojo (z = 0, dentro de la ROI), banda delante (z = 0.3, fuera de la ROI de radio 0.28) y
/// un parche detras (z = -1.5). UV: columna izquierda gris, derecha purpura.
fn eye_band_patch_source() -> &'static str {
    "v -0.5 -0.5 0\nv 0.5 -0.5 0\nv 0.5 0.5 0\nv -0.5 0.5 0\n\
     v -1 -0.1 0.3\nv 1 -0.1 0.3\nv 1 0.1 0.3\nv -1 0.1 0.3\n\
     v -0.5 -0.5 -1.5\nv 0.5 -0.5 -1.5\nv 0.5 0.5 -1.5\nv -0.5 0.5 -1.5\n\
     vt 0.25 0.5\nvt 0.75 0.5\nvn 0 0 1\n\
     f 1/1/1 2/1/1 3/1/1\nf 1/1/1 3/1/1 4/1/1\n\
     f 5/2/1 6/2/1 7/2/1\nf 5/2/1 7/2/1 8/2/1\n\
     f 9/1/1 10/1/1 11/1/1\nf 9/1/1 11/1/1 12/1/1\n"
}

fn eye_band_patch() -> Model {
    parse_obj(eye_band_patch_source()).unwrap()
}

fn synthetic_region() -> EyeRegion {
    EyeRegion::new(Vec3::ZERO, 0.28).unwrap()
}

#[test]
fn the_eye_region_is_validated_and_the_default_is_the_calibrated_asset_roi() {
    let r = CREATURE_EYE_REGION;
    assert_eq!(
        (r.center(), r.radius()),
        (Vec3::new(0.049, 0.059, 0.311), 0.20)
    );
    assert_eq!(EyeRegion::new(r.center(), r.radius()), Ok(r));
    for (center, radius) in [
        (Vec3::new(f64::NAN, 0.0, 0.0), 0.2),
        (Vec3::new(0.0, f64::INFINITY, 0.0), 0.2),
        (Vec3::ZERO, 0.0),
        (Vec3::ZERO, -0.1),
        (Vec3::ZERO, f64::NAN),
        (Vec3::ZERO, f64::INFINITY),
    ] {
        assert!(
            EyeRegion::new(center, radius).is_err(),
            "{center:?} {radius}"
        );
    }
}

#[test]
fn the_split_keeps_every_face_once_and_never_touches_the_source_geometry() {
    let model = eye_band_patch();
    let parts = StagedParts::split(&model, &synthetic_region()).unwrap();
    assert_eq!((parts.eye_faces(), parts.body_faces()), (2, 4));
    let key = |m: &Model| -> Vec<[usize; 3]> { m.faces.iter().map(|f| f.vertex_indices).collect() };
    let all = key(&model);
    assert_eq!(key(parts.eye()), all[..2].to_vec());
    assert_eq!(key(parts.body()), all[2..].to_vec());
    for part in [parts.eye(), parts.body()] {
        assert_eq!(part.vertices, model.vertices);
        assert_eq!(part.uvs, model.uvs);
        assert_eq!(part.normals, model.normals);
        assert_eq!(part.materials, model.materials);
        assert_eq!(part.bounds_center, model.bounds_center);
        assert_eq!(part.bounds_radius, model.bounds_radius);
    }
    for (sub, range) in [(parts.eye(), 0..2), (parts.body(), 2..6)] {
        for (f, i) in sub.faces.iter().zip(range) {
            assert_eq!(f.uv_indices, model.faces[i].uv_indices);
            assert_eq!(f.normal_indices, model.faces[i].normal_indices);
            assert_eq!(f.material, model.faces[i].material);
        }
    }
    // Mismos bounds: el transform de cualquier parte es el de la criatura completa.
    let placement = CreaturePlacement::demo();
    let bounds =
        |m: &Model| placement.transform_for(Vec3::from_array(m.bounds_center), m.bounds_radius);
    assert_eq!(bounds(parts.eye()), bounds(&model));
    assert_eq!(bounds(parts.body()), bounds(&model));
    // Memoria de la cache: dos copias de los arreglos de vertices, UV y normales.
    let arrays = model.vertices.len() * 24 + model.uvs.len() * 16 + model.normals.len() * 24;
    assert!(parts.cache_bytes() >= 2 * arrays, "{}", parts.cache_bytes());
}

#[test]
fn an_empty_or_total_region_fails_closed() {
    let model = eye_band_patch();
    let empty = EyeRegion::new(Vec3::new(50.0, 50.0, 50.0), 0.1).unwrap();
    let error = StagedParts::split(&model, &empty).err().unwrap();
    assert!(error.contains("ojo"), "{error}");
    let total = EyeRegion::new(Vec3::ZERO, 100.0).unwrap();
    let error = StagedParts::split(&model, &total).err().unwrap();
    assert!(error.contains("cuerpo"), "{error}");
}

// --- Compositor generalizado (compatible con V1) ---

use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::{framebuffer::Framebuffer, stats::RenderStats, texture::Texture},
    scene::world::{SPACE_COLOR, World},
    visual::{
        creature_reveal::{RevealCompositor, RevealPart},
        static_creature::StaticCreature,
    },
};

const W: usize = 320;
const H: usize = 180;

fn gray_purple_texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160],
    )
    .unwrap()
}

fn synthetic_creature() -> StaticCreature {
    StaticCreature::new(
        eye_band_patch(),
        gray_purple_texture(),
        CreaturePlacement::demo(),
    )
    .unwrap()
}

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

/// Un triangulo degenerado: oculta la nave sin tocar su camara.
fn no_ship() -> Model {
    parse_obj("v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3\n").unwrap()
}

/// Mundo con la criatura sintetica completa, la nave frente a ella e invisible.
fn band_world() -> World {
    let mut world = new_world();
    world.ship_model = no_ship();
    world.set_creature(synthetic_creature()).unwrap();
    world.place_near_creature().unwrap();
    world
}

/// Cuadro del mundo sin criatura, con su z-buffer, para dibujar encima.
fn base_frame(world: &mut World) -> (Framebuffer, RenderStats) {
    let creature = world.clear_creature();
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    if let Some(c) = creature {
        world.set_creature(c).unwrap();
    }
    (fb, stats)
}

#[test]
fn render_part_generalizes_the_compositor_and_matches_the_v1_paths_exactly() {
    let mut world = band_world();
    let light = world.system.light_position();
    let creature = world.clear_creature().unwrap();
    let transform = creature.transform();
    let part = RevealPart {
        model: creature.model(),
        transform: &transform,
        texture: creature.texture(),
    };
    let fresh = |world: &mut World| {
        let (fb, _) = base_frame(world);
        fb
    };
    // Alfa 0: nada.
    let mut fb = fresh(&mut world);
    let before = fb.pixels.clone();
    let mut compositor = RevealCompositor::new();
    let stats = compositor
        .render_part(&mut fb, part, &world.camera, light, 0.0)
        .unwrap();
    assert_eq!((fb.pixels.clone(), stats), (before, RenderStats::default()));
    // Alfa 1: igual que `StaticCreature::render` (opaco).
    let mut a = fresh(&mut world);
    let mut b = fresh(&mut world);
    let opaque = compositor
        .render_part(&mut a, part, &world.camera, light, 1.0)
        .unwrap();
    let reference = creature.render(&mut b, &world.camera, None, light).unwrap();
    assert_eq!((a.pixels, opaque), (b.pixels, reference));
    // Parcial: igual que `RevealCompositor::render` de V1 (pixeles y contadores).
    for alpha in [0.02, 0.4, 0.97] {
        let mut a = fresh(&mut world);
        let mut b = fresh(&mut world);
        let ghost = compositor
            .render_part(&mut a, part, &world.camera, light, alpha)
            .unwrap();
        let v1 = RevealCompositor::new()
            .render(&mut b, &creature, &world.camera, None, light, alpha)
            .unwrap();
        assert_eq!((a.pixels, ghost), (b.pixels, v1), "alfa {alpha}");
    }
}

// --- Integracion con World: extremos, fases, banda y contadores ---

use umbral::visual::creature_reveal::blend_rgb8;

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Fija el O2 y salta al objetivo.
fn at_oxygen(world: &mut World, o2: f64) -> (Vec<u32>, RenderStats) {
    world.debug_oxygen01 = o2;
    world.snap_creature_reveal();
    render(world)
}

/// Mundo de la banda con el modo por etapas activo y la ROI sintetica.
fn staged_band_world() -> World {
    let mut world = band_world();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), synthetic_region())
        .unwrap();
    world
}

/// Referencias con la API real en la misma pose: sin criatura y criatura estatica completa.
fn references(world: &mut World) -> ((Vec<u32>, RenderStats), (Vec<u32>, RenderStats)) {
    let (fb, base_stats) = base_frame(world);
    let mode = world.staged_creature_reveal().expect("modo por etapas");
    world.disable_staged_creature_reveal();
    let full = render(world);
    world
        .enable_staged_creature_reveal(mode.settings(), synthetic_region())
        .unwrap();
    ((fb.pixels, base_stats), full)
}

#[test]
fn staged_endpoints_match_no_creature_and_the_static_path_exactly() {
    let mut world = staged_band_world();
    let (base, full) = references(&mut world);
    assert_eq!(at_oxygen(&mut world, 1.0), base, "100 %");
    assert_eq!(
        at_oxygen(&mut world, 0.50),
        base,
        "50 %: ni pixeles ni contadores"
    );
    assert_eq!(
        at_oxygen(&mut world, 0.05),
        full,
        "5 %: ruta estatica exacta"
    );
    assert_eq!(at_oxygen(&mut world, 0.0), full);
    assert_eq!(
        world.creature_reveal_buffer_bytes(),
        0,
        "extremos sin buffers"
    );
}

#[test]
fn the_eye_comes_first_without_a_band_hole_then_the_body_blends_over_it() {
    let mut world = staged_band_world();
    let ((base, base_stats), (full, full_stats)) = references(&mut world);
    let (eye_frame, eye_stats) = at_oxygen(&mut world, 0.30);
    assert_eq!(world.staged_creature_reveal().unwrap().alphas(), (1.0, 0.0));
    let band: Vec<usize> = (0..W * H)
        .filter(|&i| eye_frame[i] != base[i] && full[i] != eye_frame[i])
        .collect();
    assert!(band.len() > 50, "el fixture necesita banda delante del ojo");

    // Solo ojo: una criatura logica, vertices del ojo (arreglos completos) y sus caras.
    let creature = world.creature().unwrap();
    let vertices = creature.model().vertices.len();
    assert_eq!(
        eye_stats.objects_submitted,
        base_stats.objects_submitted + 1
    );
    assert_eq!(
        eye_stats.vertices_processed,
        base_stats.vertices_processed + vertices
    );
    assert_eq!(
        eye_stats.triangles_submitted,
        base_stats.triangles_submitted + 2
    );

    // Fase del ojo (40 %): mezcla sobre el fondo real; la banda invisible no abre una raya.
    let (ghost, _) = at_oxygen(&mut world, 0.40);
    let (eye_alpha, body_alpha) = world.staged_creature_reveal().unwrap().alphas();
    assert!(eye_alpha > 0.0 && eye_alpha < 1.0 && body_alpha == 0.0);
    for i in 0..W * H {
        assert_eq!(
            ghost[i],
            blend_rgb8(base[i], eye_frame[i], eye_alpha),
            "pixel {i}"
        );
    }
    assert!(band.iter().all(|&i| ghost[i] != base[i]), "raya en el ojo");

    // 25 %: ojo completo y contorno muy tenue (2 %) del cuerpo, encima del ojo opaco.
    for o2 in [0.25, 0.15] {
        let (frame, stats) = at_oxygen(&mut world, o2);
        let (eye_alpha, body_alpha) = world.staged_creature_reveal().unwrap().alphas();
        assert_eq!(eye_alpha, 1.0);
        assert!(body_alpha > 0.0 && body_alpha < 1.0);
        for i in 0..W * H {
            assert_eq!(
                frame[i],
                blend_rgb8(eye_frame[i], full[i], body_alpha),
                "O2 {o2}, pixel {i}"
            );
        }
        // Una criatura logica; se ve el costo real de las dos partes.
        assert_eq!(stats.objects_submitted, full_stats.objects_submitted);
        assert_eq!(
            stats.vertices_processed,
            base_stats.vertices_processed + 2 * vertices
        );
        assert_eq!(stats.triangles_submitted, full_stats.triangles_submitted);
        if o2 == 0.15 {
            assert!(
                band.iter()
                    .all(|&i| frame[i] != eye_frame[i] && frame[i] != full[i]),
                "la banda no se mezcla sobre el ojo"
            );
        }
    }
}

#[test]
fn a_culled_staged_creature_counts_like_the_static_one_and_copies_nothing() {
    for culling in [true, false] {
        let mut world = new_world();
        world.toggles.frustum_culling = culling;
        // Detras de la camara del arranque normal.
        let behind = StaticCreature::new(
            eye_band_patch(),
            gray_purple_texture(),
            CreaturePlacement::new(Vec3::new(0.0, 40.0, 1200.0), 50.0, 0.0).unwrap(),
        )
        .unwrap();
        world.set_creature(behind).unwrap();
        world.disable_staged_creature_reveal();
        let static_frame = render(&world);
        world
            .enable_staged_creature_reveal(StagedRevealSettings::default(), synthetic_region())
            .unwrap();
        let (frame, stats) = at_oxygen(&mut world, 0.25);
        assert_eq!(frame, static_frame.0, "culling {culling}");
        if culling {
            assert_eq!(stats, static_frame.1, "contado como la estatica descartada");
            assert_eq!(world.creature_reveal_buffer_bytes(), 0, "sin copiar nada");
        } else {
            // Sin descarte paga los vertices de las dos partes y nada se rasteriza.
            assert_eq!(stats.objects_submitted, static_frame.1.objects_submitted);
            assert_eq!(
                stats.vertices_processed,
                static_frame.1.vertices_processed + 12
            );
            assert_eq!(
                stats.triangles_rasterized,
                static_frame.1.triangles_rasterized
            );
        }
    }
}

// --- Ciclo de vida: cache, exclusion con V1, snaps, tiempo real y buffers ---

use umbral::{scene::ship::ShipInput, visual::creature_reveal::RevealSettings};

#[test]
fn the_partition_cache_follows_the_creature_and_failures_change_nothing() {
    let mut world = band_world();
    let creature = world.clear_creature().unwrap();
    // Sin criatura el modo se activa sin particion y no dibuja nada.
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), synthetic_region())
        .unwrap();
    assert!(world.staged_creature_parts().is_none());
    world.set_creature(creature).unwrap();
    let parts = world
        .staged_creature_parts()
        .expect("particion al poner la criatura");
    assert_eq!((parts.eye_faces(), parts.body_faces()), (2, 4));

    // Un modelo cuya region lo toma entero falla antes del cambio: todo sigue igual.
    let tiny = StaticCreature::new(
        parse_obj("v -0.01 0 0\nv 0.01 0 0\nv 0 0.01 0\nf 1 2 3\n").unwrap(),
        gray_purple_texture(),
        CreaturePlacement::demo(),
    )
    .unwrap();
    let before = render(&world);
    let error = world.set_creature(tiny).expect_err("debe fallar cerrado");
    assert!(error.contains("cuerpo"), "{error}");
    assert_eq!(world.creature().unwrap().model().faces.len(), 6);
    assert_eq!(world.staged_creature_parts().unwrap().eye_faces(), 2);
    assert_eq!(render(&world), before);

    // Quitar la criatura suelta la cache; el modo sigue.
    let creature = world.clear_creature().unwrap();
    assert!(world.staged_creature_parts().is_none() && world.staged_creature_reveal().is_some());
    world.set_creature(creature).unwrap();
    assert!(world.staged_creature_parts().is_some());

    // Activar con una region que no sirve: `Err` y el mundo queda como estaba.
    world.enable_creature_reveal(RevealSettings::default());
    let empty = EyeRegion::new(Vec3::splat(50.0), 0.1).unwrap();
    assert!(
        world
            .enable_staged_creature_reveal(StagedRevealSettings::default(), empty)
            .is_err()
    );
    assert!(world.staged_creature_reveal().is_none() && world.creature_reveal().is_some());
}

#[test]
fn v1_and_staged_modes_exclude_each_other_and_disable_returns_the_static_creature() {
    let mut world = band_world();
    world.debug_oxygen01 = 0.25;
    let static_frame = render(&world);
    world.enable_creature_reveal(RevealSettings::default());
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), synthetic_region())
        .unwrap();
    assert!(world.creature_reveal().is_none(), "etapas sustituye a V1");
    world.enable_creature_reveal(RevealSettings::default());
    assert!(
        world.staged_creature_reveal().is_none(),
        "V1 sustituye a etapas"
    );
    world.disable_creature_reveal();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), synthetic_region())
        .unwrap();
    world.disable_staged_creature_reveal();
    assert!(world.staged_creature_parts().is_none());
    assert_eq!(render(&world), static_frame);
}

#[test]
fn staged_transitions_use_real_time_and_resets_keep_oxygen_and_snap() {
    let s = StagedRevealSettings::default();
    let mut world = staged_band_world();
    world.debug_oxygen01 = 1.0;
    world.snap_creature_reveal();
    world.debug_oxygen01 = 0.05;
    // Ni la fisica ni F5 mueven el efecto; solo el paso visual con segundos reales.
    world.toggles.freeze_orbits = true;
    world.update(&ShipInput::default(), 0.1);
    assert_eq!(world.staged_creature_reveal().unwrap().shown(), 0.0);
    let time = world.time;
    world.update_creature_reveal(2.0);
    assert!((world.staged_creature_reveal().unwrap().shown() - 0.95).abs() < 1e-12);
    assert_eq!(world.time, time, "no suma al tiempo orbital");
    // R (con o sin --near-creature) y el arranque junto a un cuerpo conservan el O2 y saltan.
    world.debug_oxygen01 = 0.25;
    world.place_near_creature().unwrap();
    assert_eq!(world.debug_oxygen01, 0.25);
    assert_eq!(
        world.staged_creature_reveal().unwrap().alphas(),
        (1.0, 0.02)
    );
    world.debug_oxygen01 = 0.40;
    world.reset_ship();
    assert_eq!(
        world.staged_creature_reveal().unwrap().shown(),
        staged_target(0.40, &s)
    );
    world.debug_oxygen01 = 0.05;
    world.place_near(1, 25.0).unwrap();
    assert_eq!(world.staged_creature_reveal().unwrap().alphas(), (1.0, 1.0));
    // La malla, el transform y la textura de la criatura nunca cambian.
    let transform = world.creature().unwrap().transform();
    let vertices = world.creature().unwrap().model().vertices.clone();
    for _ in 0..20 {
        world.update_creature_reveal(1.0 / 60.0);
        let _ = render(&world);
    }
    assert_eq!(world.creature().unwrap().transform(), transform);
    assert_eq!(world.creature().unwrap().model().vertices, vertices);
}

#[test]
fn staged_buffers_are_reused_and_follow_the_resolution() {
    let mut world = staged_band_world();
    let first = at_oxygen(&mut world, 0.20);
    let bytes = world.creature_reveal_buffer_bytes();
    assert!(bytes >= W * H * 5, "{bytes}");
    for _ in 0..3 {
        assert_eq!(render(&world), first);
    }
    assert_eq!(world.creature_reveal_buffer_bytes(), bytes, "sin crecer");
    // Otra resolucion: correcto con buffers ajustados y de vuelta igual.
    let mut small = Framebuffer::new(160, 90, SPACE_COLOR).unwrap();
    let _ = world.render(&mut small);
    assert_eq!(render(&world), first);
}

// --- Linea de comandos ---

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[test]
fn staged_flags_are_validated_before_any_io() {
    assert_eq!(validate_staged_flags(false, false, false, false), Ok(()));
    assert_eq!(validate_staged_flags(true, true, false, false), Ok(()));
    for (stages, creature, uniform, legacy, needle) in [
        (
            true,
            false,
            false,
            false,
            "--creature-reveal-stages necesita",
        ),
        (true, true, true, false, "incompatibles"),
        (true, false, false, true, "--legacy"),
        (true, true, true, true, "--legacy"),
    ] {
        let error = validate_staged_flags(stages, creature, uniform, legacy)
            .expect_err(&format!("aceptado: {stages} {creature} {uniform} {legacy}"));
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-stages-{name}-{}", std::process::id()));
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

/// OBJ sintetico con un ojo pequeno dentro de la ROI calibrada del asset real (la que usa la
/// linea de comandos), una banda delante y un parche detras fuera de ella.
fn calibrated_source() -> String {
    let c = CREATURE_EYE_REGION.center();
    let v = |x: f64, y: f64, z: f64| format!("v {} {} {}\n", c.x + x, c.y + y, c.z + z);
    let mut s = String::new();
    for (x, y) in [(-0.1, -0.1), (0.1, -0.1), (0.1, 0.1), (-0.1, 0.1)] {
        s += &v(x, y, 0.0);
    }
    for (x, y) in [(-1.0, -0.05), (1.0, -0.05), (1.0, 0.05), (-1.0, 0.05)] {
        s += &v(x, y, 0.3);
    }
    for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
        s += &v(x, y, -1.5);
    }
    s + "vt 0.25 0.5\nvt 0.75 0.5\nvn 0 0 1\n\
         f 1/1/1 2/1/1 3/1/1\nf 1/1/1 3/1/1 4/1/1\n\
         f 5/2/1 6/2/1 7/2/1\nf 5/2/1 7/2/1 8/2/1\n\
         f 9/1/1 10/1/1 11/1/1\nf 9/1/1 11/1/1 12/1/1\n"
}

/// Siempre con `--headless`: estas pruebas nunca abren ventana.
fn run_game(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_game_command_line_wires_the_staged_reveal_and_rejects_bad_combinations() {
    let dir = temp_dir("cli");
    let (model, texture) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    write_texture(&texture);
    let far = dir.join("lejos.obj");
    std::fs::write(
        &far,
        "v 5 5 5\nv 6 5 5\nv 5 6 5\nv 5 5 6\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n",
    )
    .unwrap();
    let texture_bytes = std::fs::read(&texture).unwrap();
    let output = dir.join("salida.png");
    let os = OsStr::new;
    let outcome = std::panic::catch_unwind(|| {
        let creature = [
            os("--creature-model"),
            model.as_os_str(),
            os("--creature-texture"),
            texture.as_os_str(),
        ];
        let missing = [
            os("--creature-model"),
            os("no-existe.obj"),
            os("--creature-texture"),
            os("no-existe.png"),
        ];
        let failing: Vec<(Vec<&OsStr>, &str)> = vec![
            (
                vec![os("--creature-reveal-stages")],
                "--creature-reveal-stages necesita",
            ),
            (
                vec![os("--legacy"), os("--creature-reveal-stages")],
                "--legacy",
            ),
            // Rutas inexistentes: el error es de validacion, no de IO.
            (
                [
                    &missing[..],
                    &[os("--creature-reveal"), os("--creature-reveal-stages")],
                ]
                .concat(),
                "incompatibles",
            ),
            // ROI calibrada sin ninguna cara en otro modelo: falla cerrado al cargar.
            (
                vec![
                    os("--creature-model"),
                    far.as_os_str(),
                    os("--creature-texture"),
                    texture.as_os_str(),
                    os("--creature-reveal-stages"),
                ],
                "no contiene ninguna cara",
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
            texture.as_os_str(),
            os("--creature-reveal-stages"),
        ];
        args.extend(creature);
        let result = run_game(&args);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("coincide con la entrada"));
        assert_eq!(std::fs::read(&texture).unwrap(), texture_bytes);

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
        let near = [
            &creature[..],
            &[os("--near-creature"), os("--creature-reveal-stages")],
        ]
        .concat();
        // 25 %: ojo completo y contorno de 2 % desde el primer cuadro.
        let o25 = [&near[..], &[os("--debug-oxygen"), os("0.25")]].concat();
        let (_, stdout) = capture("o25.png", &o25);
        assert!(
            stdout.contains(
                "Revelacion por etapas: O2 0.25 | ojo 100% cuerpo 2.0% | caras ojo 2 cuerpo 4"
            ),
            "{stdout}"
        );
        assert!(stdout.contains("objetos 6 (descartados"), "{stdout}");
        // 100 % (por defecto): oculta, ni se dibuja ni se cuenta.
        let (_, stdout) = capture("o100.png", &near);
        assert!(stdout.contains("ojo 0% cuerpo 0.0%"), "{stdout}");
        assert!(stdout.contains("objetos 5 (descartados"), "{stdout}");
        // Sin --near-creature no mueve la nave, y al 5 % es la estatica exacta.
        let full_args = [
            &creature[..],
            &[
                os("--creature-reveal-stages"),
                os("--debug-oxygen"),
                os("0.05"),
            ],
        ]
        .concat();
        let (full, _) = capture("full.png", &full_args);
        let (static_frame, _) = capture("static.png", &creature);
        assert_eq!(full, static_frame);
    });
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}
