//! Visor orbital de la criatura: control de camara puro, contrato de bounds, HUD y ruta de
//! salida. Fixtures sinteticos y pequenos; no necesita el asset real ni abre ventanas.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use umbral::{
    assets::obj::{Model, parse_obj},
    math::{Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer, pipeline::draw_model_with, shader::Material, stats::RenderStats,
        texture::Texture,
    },
    scene::world::SPACE_COLOR,
    visual::{
        creature_viewer::*,
        textured::{TexturedMode, TexturedShader},
    },
};

fn pose(yaw: f64, pitch: f64, distance: f64) -> OrbitPose {
    OrbitPose::new(yaw, pitch, distance).expect("pose valida")
}

fn input(yaw: f64, pitch: f64, zoom: f64) -> OrbitInput {
    OrbitInput { yaw, pitch, zoom }
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn close_vec(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    (a - b).length() <= tolerance
}

#[test]
fn default_pose_matches_preview_framing() {
    let p = OrbitPose::default();
    assert_eq!(p.yaw(), 0.0);
    assert_eq!(p.pitch(), 0.15_f64.atan());
    assert_eq!(
        p.distance_radii(),
        FRAME_MARGIN / (FOV_DEGREES.to_radians() / 2.0).sin()
    );
    assert!((MIN_DISTANCE_RADII..=MAX_DISTANCE_RADII).contains(&p.distance_radii()));
    // Misma direccion que la camara del preview estatico: +Z del OBJ, ligeramente elevada.
    let expected = Vec3::new(0.0, 0.15, 1.0).normalized();
    assert!(
        close_vec(p.direction(), expected, 1e-15),
        "{:?}",
        p.direction()
    );
}

#[test]
fn pose_rejects_nan_infinite_and_out_of_range_values() {
    let invalid = [
        (f64::NAN, 0.0, 3.0),
        (f64::INFINITY, 0.0, 3.0),
        (0.0, f64::NAN, 3.0),
        (0.0, f64::NEG_INFINITY, 3.0),
        (0.0, MAX_PITCH + 1e-9, 3.0),
        (0.0, -FRAC_PI_2, 3.0),
        (0.0, 0.0, f64::NAN),
        (0.0, 0.0, f64::INFINITY),
        (0.0, 0.0, MIN_DISTANCE_RADII - 1e-9),
        (0.0, 0.0, MAX_DISTANCE_RADII + 1e-9),
        (0.0, 0.0, 0.0),
        (0.0, 0.0, -3.0),
    ];
    for (yaw, pitch, distance) in invalid {
        let error = OrbitPose::new(yaw, pitch, distance)
            .expect_err(&format!("aceptada: {yaw} {pitch} {distance}"));
        assert!(!error.is_empty());
    }
    // Los extremos del rango son validos y el pitch no alcanza los polos.
    const { assert!(MAX_PITCH < FRAC_PI_2) };
    for (pitch, distance) in [
        (MAX_PITCH, MIN_DISTANCE_RADII),
        (-MAX_PITCH, MAX_DISTANCE_RADII),
    ] {
        let p = pose(0.0, pitch, distance);
        assert_eq!((p.pitch(), p.distance_radii()), (pitch, distance));
    }
    // Yaw finito cualquiera se envuelve a [-pi, pi]; dentro del rango se conserva exacto.
    assert_eq!(pose(0.8, 0.0, 3.0).yaw(), 0.8);
    assert_eq!(pose(-PI, 0.0, 3.0).yaw(), -PI);
    assert!(close(pose(7.0, 0.0, 3.0).yaw(), 7.0 - TAU, 1e-12));
    for yaw in [7.0, -7.0, 1e300, -1e300] {
        let wrapped = pose(yaw, 0.0, 3.0).yaw();
        assert!((-PI..=PI).contains(&wrapped), "{yaw} -> {wrapped}");
    }
}

#[test]
fn arrows_orbit_with_documented_sign_and_rate() {
    assert_eq!(
        OrbitInput::from_keys(false, true, false, false, false, false),
        input(1.0, 0.0, 0.0)
    );
    assert_eq!(
        OrbitInput::from_keys(true, false, false, false, false, false),
        input(-1.0, 0.0, 0.0)
    );
    assert_eq!(
        OrbitInput::from_keys(true, true, true, true, true, true),
        OrbitInput::NONE
    );
    assert_eq!(
        OrbitInput::from_keys(false, false, true, false, true, false),
        input(0.0, 1.0, 1.0)
    );
    assert_eq!(
        OrbitInput::from_keys(false, false, false, true, false, true),
        input(0.0, -1.0, -1.0)
    );

    let mut controller = OrbitController::new(pose(0.0, 0.0, 3.0));
    controller.step(input(1.0, 0.0, 0.0), 0.05).unwrap();
    let p = controller.pose();
    assert!(close(p.yaw(), YAW_RATE * 0.05, 1e-15));
    assert_eq!((p.pitch(), p.distance_radii()), (0.0, 3.0));
    // Flecha derecha: la camara se mueve hacia su derecha (+X a yaw 0).
    assert!(p.direction().x > 0.0);

    controller.step(input(0.0, 1.0, 0.0), 0.05).unwrap();
    assert!(close(controller.pose().pitch(), PITCH_RATE * 0.05, 1e-15));
    assert!(controller.pose().direction().y > 0.0);

    // El yaw acumulado se mantiene envuelto.
    let mut controller = OrbitController::new(pose(3.1, 0.0, 3.0));
    controller.step(input(1.0, 0.0, 0.0), 0.1).unwrap();
    let yaw = controller.pose().yaw();
    assert!((-PI..=PI).contains(&yaw));
    assert!(close(yaw, 3.1 + YAW_RATE * 0.1 - TAU, 1e-12));
}

#[test]
fn pitch_is_clamped_and_never_crosses_the_poles() {
    let mut controller = OrbitController::new(OrbitPose::default());
    for _ in 0..1000 {
        controller.step(input(0.3, 1.0, 0.0), MAX_DT).unwrap();
    }
    assert_eq!(controller.pose().pitch(), MAX_PITCH);
    let d = controller.pose().direction();
    assert!(close(d.length(), 1.0, 1e-12));
    // Componente horizontal no nula: la vista nunca queda paralela al eje Y.
    assert!(d.x.hypot(d.z) >= MAX_PITCH.cos() - 1e-12);
    for _ in 0..1000 {
        controller.step(input(-0.3, -1.0, 0.0), MAX_DT).unwrap();
    }
    assert_eq!(controller.pose().pitch(), -MAX_PITCH);
}

#[test]
fn w_moves_closer_and_distance_stays_outside_the_sphere() {
    const { assert!(MIN_DISTANCE_RADII > 1.0) };
    let mut controller = OrbitController::new(pose(0.0, 0.0, 3.0));
    controller.step(input(0.0, 0.0, 1.0), 0.1).unwrap();
    let closer = controller.pose().distance_radii();
    assert!(close(closer, 3.0 * (-ZOOM_RATE * 0.1).exp(), 1e-12));
    assert!(closer < 3.0);
    controller.step(input(0.0, 0.0, -1.0), 0.1).unwrap();
    assert!(close(controller.pose().distance_radii(), 3.0, 1e-12));
    for _ in 0..1000 {
        controller.step(input(0.0, 0.0, 1.0), MAX_DT).unwrap();
    }
    assert_eq!(controller.pose().distance_radii(), MIN_DISTANCE_RADII);
    for _ in 0..1000 {
        controller.step(input(0.0, 0.0, -1.0), MAX_DT).unwrap();
    }
    assert_eq!(controller.pose().distance_radii(), MAX_DISTANCE_RADII);
}

#[test]
fn invalid_dt_or_input_is_rejected_without_changing_state_and_pauses_are_capped() {
    let mut controller = OrbitController::new(pose(0.3, 0.2, 4.0));
    let before = controller.pose();
    for dt in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1e-9, -1.0] {
        assert!(
            controller.step(input(1.0, 1.0, 1.0), dt).is_err(),
            "dt {dt}"
        );
        assert_eq!(controller.pose(), before);
    }
    for bad in [
        input(f64::NAN, 0.0, 0.0),
        input(0.0, 1.5, 0.0),
        input(0.0, 0.0, f64::NEG_INFINITY),
        input(-1.0 - 1e-9, 0.0, 0.0),
    ] {
        assert!(controller.step(bad, 0.01).is_err(), "{bad:?}");
        assert_eq!(controller.pose(), before);
    }
    controller.step(input(1.0, 1.0, 1.0), 0.0).unwrap();
    assert_eq!(controller.pose(), before);

    // Una pausa larga avanza como MAX_DT, no como el tiempo real transcurrido.
    let mut capped = OrbitController::new(before);
    capped.step(input(1.0, -1.0, 1.0), MAX_DT).unwrap();
    for dt in [1000.0, f64::MAX] {
        let mut paused = OrbitController::new(before);
        paused.step(input(1.0, -1.0, 1.0), dt).unwrap();
        assert_eq!(paused.pose(), capped.pose(), "dt {dt}");
    }
}

#[test]
fn reset_restores_the_initial_pose_exactly() {
    let initial = pose(-0.7, 0.33, 5.5);
    let mut controller = OrbitController::new(initial);
    for k in 0..37 {
        let s = if k % 3 == 0 { -1.0 } else { 1.0 };
        controller
            .step(input(s, 0.7 * s, -0.4 * s), 0.013 * k as f64)
            .unwrap();
    }
    assert_ne!(controller.pose(), initial);
    controller.reset();
    let p = controller.pose();
    assert_eq!(p, initial);
    assert_eq!(p.yaw().to_bits(), initial.yaw().to_bits());
    assert_eq!(p.pitch().to_bits(), initial.pitch().to_bits());
    assert_eq!(
        p.distance_radii().to_bits(),
        initial.distance_radii().to_bits()
    );
}

#[test]
fn camera_orbits_a_displaced_center_and_scales_with_the_radius() {
    let p = pose(0.9, -0.4, 2.5);
    let displaced = CreatureBounds::new(Vec3::new(1.0e6, -3.0e5, 2.5e5), 0.5).unwrap();
    let large = CreatureBounds::new(Vec3::ZERO, 40.0).unwrap();
    for bounds in [displaced, large] {
        let (center, radius) = (bounds.center(), bounds.radius());
        let camera = bounds.camera(&p, 16.0 / 9.0).unwrap();
        assert_eq!(camera.target, center);
        assert_eq!(camera.fov_y, FOV_DEGREES.to_radians());
        let offset = (camera.position - center) * (1.0 / radius);
        assert!(
            close_vec(offset, p.direction() * 2.5, 1e-8),
            "offset en radios {offset:?}"
        );
        // El plano cercano queda delante de la esfera y el lejano detras.
        assert!(camera.near > 0.0 && camera.near < (2.5 - 1.0) * radius);
        assert!(camera.far > (2.5 + 1.0) * radius);
        let vp = camera.view_projection();
        assert!(vp.m.iter().flatten().all(|v| v.is_finite()));
        // El centro envolvente se proyecta en el centro de la imagen.
        let c = vp.transform(center.extend(1.0));
        assert!(close(c.x / c.w, 0.0, 1e-9) && close(c.y / c.w, 0.0, 1e-9));
        assert!(close_vec(
            bounds.light_position(),
            center + Vec3::new(-1.5, 2.0, 3.0) * radius,
            1e-9
        ));
    }
}

#[test]
fn bounds_contract_rejects_invalid_values_and_extremes_stay_finite() {
    let invalid = [
        (Vec3::new(f64::NAN, 0.0, 0.0), 1.0),
        (Vec3::new(0.0, f64::INFINITY, 0.0), 1.0),
        (Vec3::new(0.0, 0.0, MAX_CENTER_COORD * 1.5), 1.0),
        (Vec3::ZERO, f64::NAN),
        (Vec3::ZERO, f64::INFINITY),
        (Vec3::ZERO, 0.0),
        (Vec3::ZERO, -1.0),
        (Vec3::ZERO, MIN_RADIUS * 0.5),
        (Vec3::ZERO, MAX_RADIUS * 2.0),
        // Radio despreciable frente al desplazamiento: la precision f64 no alcanza.
        (Vec3::splat(MAX_CENTER_COORD), 0.5),
    ];
    for (center, radius) in invalid {
        assert!(
            CreatureBounds::new(center, radius).is_err(),
            "aceptado: {center:?} r {radius}"
        );
    }
    let extremes = [
        CreatureBounds::new(Vec3::splat(MAX_CENTER_COORD), MAX_RADIUS).unwrap(),
        CreatureBounds::new(Vec3::splat(-MAX_CENTER_COORD), MAX_RADIUS).unwrap(),
        CreatureBounds::new(Vec3::ZERO, MIN_RADIUS).unwrap(),
    ];
    let poses = [
        pose(PI, MAX_PITCH, MAX_DISTANCE_RADII),
        pose(-PI, -MAX_PITCH, MIN_DISTANCE_RADII),
        OrbitPose::default(),
    ];
    for bounds in extremes {
        for p in poses {
            let camera = bounds.camera(&p, 16.0 / 9.0).unwrap();
            assert!(camera.position.is_finite() && camera.near.is_finite());
            assert!(camera.far.is_finite() && camera.far > camera.near);
            let vp = camera.view_projection();
            assert!(
                vp.m.iter().flatten().all(|v| v.is_finite()),
                "{bounds:?} {p:?}"
            );
            assert!(bounds.light_position().is_finite());
        }
    }
    let bounds = CreatureBounds::new(Vec3::ZERO, 1.0).unwrap();
    for aspect in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(bounds.camera(&OrbitPose::default(), aspect).is_err());
    }
    let model = parse_obj("v 9 0 0\nv 11 0 0\nv 11 2 0\nv 9 2 0\nf 1 2 3\nf 1 3 4\n").unwrap();
    let from_model = CreatureBounds::from_model(&model).unwrap();
    assert_eq!(from_model.center(), Vec3::from_array(model.bounds_center));
    assert_eq!(from_model.radius(), model.bounds_radius);
}

#[test]
fn hud_is_deterministic_headless_and_separates_render_from_presentation() {
    let p = OrbitPose::default();
    let lit = hud_lines(TexturedMode::Lit, (1024, 1024), &p, None);
    assert_eq!(lit, hud_lines(TexturedMode::Lit, (1024, 1024), &p, None));
    let text = lit.join("\n");
    for needle in [
        "LIT",
        "1024X1024",
        "3.22",
        "FLECHAS",
        "W/S",
        "R ",
        "L ",
        "P ",
        "ESC",
    ] {
        assert!(text.contains(needle), "falta {needle:?} en {text}");
    }
    assert!(!text.contains("FPS") && !text.contains(" MS"), "{text}");
    let unlit = hud_lines(TexturedMode::Unlit, (4096, 4096), &p, None).join("\n");
    assert!(unlit.contains("UNLIT") && unlit.contains("4096X4096"));

    let timing = HudTiming {
        render_ms: 45.26,
        presented_fps: 14.8,
    };
    let live = hud_lines(TexturedMode::Lit, (1024, 1024), &p, Some(timing)).join("\n");
    assert!(live.contains("RENDER CPU 45.3 MS"), "{live}");
    assert!(live.contains("PRESENTADOS 14.8 FPS"), "{live}");
    assert!(live.contains(&format!("TOPE {TARGET_FPS}")), "{live}");
    let unknown = HudTiming {
        render_ms: f64::NAN,
        presented_fps: f64::INFINITY,
    };
    let unknown = hud_lines(TexturedMode::Lit, (1, 1), &p, Some(unknown)).join("\n");
    assert!(!unknown.contains("NAN") && !unknown.contains("INF") && unknown.contains("--"));

    // Solo caracteres de la fuente del overlay y cabe a 1280 de ancho con escala 2.
    for line in hud_lines(TexturedMode::Unlit, (4096, 4096), &p, Some(timing)) {
        assert!(
            line.chars().all(|c| c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || " .:/%()=+,|-".contains(c)),
            "{line}"
        );
        assert!(line.chars().count() * 12 + 16 <= 1280, "{line}");
    }
}

#[test]
fn output_must_be_png_and_must_not_overwrite_an_input() {
    let dir = std::env::temp_dir().join(format!("umbral-viewer-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("modelo.obj");
    let texture = dir.join("textura.png");
    let other = dir.join("otra.png");
    for path in [&model, &texture, &other] {
        std::fs::write(path, b"x").unwrap();
    }
    let inputs = [model.as_path(), texture.as_path()];
    let result = || {
        assert!(validate_output_path(&texture, &inputs).is_err());
        assert!(validate_output_path(&dir.join(".").join("textura.png"), &inputs).is_err());
        assert!(validate_output_path(&dir.join("salida.obj"), &inputs).is_err());
        assert!(validate_output_path(&dir.join("salida"), &inputs).is_err());
        assert!(validate_output_path(&dir.join("salida.png"), &inputs).is_ok());
        assert!(validate_output_path(&dir.join("salida.PNG"), &inputs).is_ok());
        assert!(validate_output_path(&other, &inputs).is_ok());
    };
    let outcome = std::panic::catch_unwind(result);
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}

// --- Render headless integrado (fixture sintetico: cubo texturizado y desplazado) ---

const W: usize = 160;
const H: usize = 90;

/// Cubo de lado 4 centrado en (50, -20, 7); cada cara usa todo el rango UV.
fn cube() -> Model {
    let (c, h) = (Vec3::new(50.0, -20.0, 7.0), 2.0);
    let mut source = String::new();
    for (x, y, z) in [
        (-1.0, -1.0, -1.0),
        (1.0, -1.0, -1.0),
        (1.0, 1.0, -1.0),
        (-1.0, 1.0, -1.0),
        (-1.0, -1.0, 1.0),
        (1.0, -1.0, 1.0),
        (1.0, 1.0, 1.0),
        (-1.0, 1.0, 1.0),
    ] {
        source += &format!("v {} {} {}\n", c.x + x * h, c.y + y * h, c.z + z * h);
    }
    source += "vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n";
    for [a, b, cc, d] in [
        [5, 6, 7, 8],
        [2, 1, 4, 3],
        [6, 2, 3, 7],
        [1, 5, 8, 4],
        [8, 7, 3, 4],
        [1, 2, 6, 5],
    ] {
        source += &format!("f {a}/1 {b}/2 {cc}/3\nf {a}/1 {cc}/3 {d}/4\n");
    }
    parse_obj(&source).unwrap()
}

/// 2x2 con cuatro colores vivos, distintos del fondo.
fn texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![230, 40, 40, 40, 220, 60, 50, 80, 240, 240, 220, 40],
    )
    .unwrap()
}

fn render(
    model: &Model,
    tex: &Texture,
    p: &OrbitPose,
    mode: TexturedMode,
) -> (Framebuffer, RenderStats) {
    let bounds = CreatureBounds::from_model(model).unwrap();
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = render_view(&mut fb, model, tex, &bounds, p, mode).unwrap();
    (fb, stats)
}

/// Pixeles con profundidad finita (z-buffer) y comprobacion de rango NDC.
fn covered(fb: &Framebuffer) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let (xi, yi) = (x as isize, y as isize);
            let finite = !fb.depth_passes(xi, yi, f64::MAX);
            let drawn = fb.pixels[y * W + x] != SPACE_COLOR;
            assert_eq!(finite, drawn, "z-buffer y color discrepan en ({x}, {y})");
            if finite {
                assert!(
                    fb.depth_passes(xi, yi, -1.0 - 1e-12) && !fb.depth_passes(xi, yi, 1.0 + 1e-12)
                );
                out.push((x, y));
            }
        }
    }
    out
}

#[test]
fn render_view_is_the_shared_pipeline_and_textured_shader() {
    let (model, tex) = (cube(), texture());
    let bounds = CreatureBounds::from_model(&model).unwrap();
    let p = pose(0.6, 0.25, 3.0);
    for mode in [TexturedMode::Lit, TexturedMode::Unlit] {
        // Se dibuja sobre un framebuffer sucio (otra pose): render_view debe limpiarlo.
        let (mut fb, _) = render(&model, &tex, &pose(-1.0, -0.5, 6.0), mode);
        let stats = render_view(&mut fb, &model, &tex, &bounds, &p, mode).unwrap();

        let mut expected = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
        let camera = bounds.camera(&p, W as f64 / H as f64).unwrap();
        let shader = TexturedShader::new(&tex, mode, bounds.light_position(), AMBIENT).unwrap();
        let expected_stats = draw_model_with(
            &mut expected,
            &model,
            &Transform::IDENTITY,
            &Material::from_model(),
            &camera,
            |f, _| shader.shade(f),
        );
        assert_eq!(stats, expected_stats, "{mode:?}");
        assert!(fb.pixels == expected.pixels, "{mode:?}: imagen distinta");
        assert!(stats.fragments_evaluated > 0 && stats.triangles_rasterized > 0);
        assert_eq!(stats.triangles_backface, 0, "sin culling: malla no cerrada");
    }
}

#[test]
fn poses_give_distinct_repeatable_images_with_a_finite_z_buffer() {
    let (model, tex) = (cube(), texture());
    let poses = [
        OrbitPose::default(),
        pose(0.9, 0.0, 3.0),
        pose(0.0, -0.6, 3.0),
        pose(0.0, 0.0, MIN_DISTANCE_RADII),
        pose(0.0, 0.0, MAX_DISTANCE_RADII),
        pose(PI, MAX_PITCH, 4.0),
    ];
    let mut images: Vec<Vec<u32>> = Vec::new();
    for p in &poses {
        let (a, stats) = render(&model, &tex, p, TexturedMode::Lit);
        let (b, again) = render(&model, &tex, p, TexturedMode::Lit);
        assert!(
            a.pixels == b.pixels && stats == again,
            "no repetible: {p:?}"
        );
        let pixels = covered(&a);
        assert!(!pixels.is_empty(), "nada visible en {p:?}");
        assert!(stats.pixels_written >= pixels.len());
        for (other, image) in poses.iter().zip(&images) {
            assert!(*image != a.pixels, "{p:?} y {other:?} dan la misma imagen");
        }
        images.push(a.pixels);
    }
    let (unlit, _) = render(&model, &tex, &OrbitPose::default(), TexturedMode::Unlit);
    assert!(unlit.pixels != images[0], "Lit y Unlit iguales");

    // La camara apunta al centro envolvente aunque el modelo este lejos del origen.
    let (front, _) = render(&model, &tex, &pose(0.0, 0.0, 3.0), TexturedMode::Unlit);
    let pixels = covered(&front);
    let n = pixels.len() as f64;
    let cx = pixels.iter().map(|p| p.0 as f64 + 0.5).sum::<f64>() / n;
    let cy = pixels.iter().map(|p| p.1 as f64 + 0.5).sum::<f64>() / n;
    assert!(
        close(cx, W as f64 / 2.0, 1.0) && close(cy, H as f64 / 2.0, 1.0),
        "centroide ({cx}, {cy})"
    );
    // Mas cerca, mas grande.
    let (near, _) = render(&model, &tex, &pose(0.0, 0.0, 2.0), TexturedMode::Unlit);
    assert!(covered(&near).len() > pixels.len());
}
