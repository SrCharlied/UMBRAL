//! Supervisor tracer: full blackout belongs to zero oxygen (or the explicit attack sample).
#[path = "../src/visual/lucid_perception_variant.rs"]
mod variant;

use umbral::renderer::framebuffer::Framebuffer;

#[test]
fn zero_oxygen_is_a_complete_blackout_not_the_previous_peripheral_effect() {
    let source = Framebuffer::new(320, 180, 0x8090a0).unwrap();
    let output = variant::lucid_perception_variant(&source, 0.0, 1.3, [0.5, 0.63], false).unwrap();
    assert_eq!(output.len(), source.pixels.len());
    assert!(
        output.iter().all(|&p| p == 0),
        "zero oxygen must produce a full blackout, not the previous readable-center effect"
    );
    assert_eq!(source.pixels, vec![0x8090a0; 320 * 180]);
}

// --- Bronya: mecanismos de la variante 02 (RED/GREEN y guards en ../worker-evidence) ---

use variant::lucid_perception_variant as perceive;

/// Foco de prueba deliberadamente lejos del centro geometrico (la nave suele ir mas abajo).
const FOCUS: [f64; 2] = [0.3, 0.7];

/// Imagen con color distinto en cada pixel.
fn varied(width: usize, height: usize) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, 0).unwrap();
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width.max(2).saturating_sub(1).max(1)) as u32 & 0xff;
            let g = (y * 255 / height.max(2).saturating_sub(1).max(1)) as u32 & 0xff;
            let b = ((x * 7 + y * 13) % 256) as u32;
            fb.pixels[y * width + x] = (r << 16) | (g << 8) | b;
        }
    }
    fb
}

fn rgb(p: u32) -> [i32; 3] {
    [
        (p >> 16) as i32 & 0xff,
        (p >> 8) as i32 & 0xff,
        p as i32 & 0xff,
    ]
}

fn max_delta(a: &[u32], b: &[u32]) -> i32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .flat_map(|(&x, &y)| {
            let (x, y) = (rgb(x), rgb(y));
            (0..3).map(move |i| (x[i] - y[i]).abs())
        })
        .max()
        .unwrap_or(0)
}

/// Indice del pixel en coordenadas de pantalla `[0, 1]`.
fn at(width: usize, height: usize, uv: [f64; 2]) -> usize {
    let x = ((uv[0] * width as f64) as usize).min(width - 1);
    let y = ((uv[1] * height as f64) as usize).min(height - 1);
    y * width + x
}

#[test]
fn an_explicit_attack_sample_blacks_out_even_with_full_oxygen() {
    let source = varied(320, 180);
    for oxygen in [1.0, 0.5, 0.25, 0.05] {
        for time in [0.0, 1.3, 9.0] {
            let output = perceive(&source, oxygen, time, FOCUS, true).unwrap();
            assert!(
                output.iter().all(|&p| p == 0),
                "ataque con O2 {oxygen} t {time}"
            );
            assert_eq!(output.len(), source.pixels.len());
        }
    }
}

#[test]
fn high_oxygen_without_attack_keeps_every_pixel_exact() {
    let source = varied(320, 180);
    for oxygen in [1.0, 0.8, 0.6, variant::PERCEPTION_ONSET_OXYGEN] {
        for time in [0.0, 1.3, 1.0e9] {
            for focus in [FOCUS, [0.5, 0.5], [0.0, 1.0]] {
                let output = perceive(&source, oxygen, time, focus, false).unwrap();
                assert_eq!(output, source.pixels, "O2 {oxygen} t {time} foco {focus:?}");
            }
        }
    }
}

fn luma(c: [i32; 3]) -> f64 {
    0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64
}

/// Cuanto domina el violeta (R y B sobre G).
fn violet(c: [i32; 3]) -> i32 {
    c[0].min(c[2]) - c[1]
}

#[test]
fn low_oxygen_turns_the_scene_grey_violet_without_painting_the_void() {
    let (w, h) = (320, 180);
    for color in [0x00c800, 0xdc8c28, 0x808080, 0x3060d0] {
        let source = Framebuffer::new(w, h, color).unwrap();
        let input = rgb(color);
        let critical = rgb(perceive(&source, 0.05, 1.3, FOCUS, false).unwrap()[at(w, h, FOCUS)]);
        // Tono violeta inequivoco y desaturado: ningun canal original domina sobre R/B.
        assert!(violet(critical) >= 20, "{color:06x} -> {critical:?}");
        assert!(
            critical[1] < critical[0] && critical[1] < critical[2],
            "{critical:?}"
        );
        // Gris + morado conserva la luminancia: no es oscurecimiento disfrazado.
        let (li, lo) = (luma(input), luma(critical));
        assert!(
            (lo - li).abs() <= li * 0.12 + 3.0,
            "{color:06x}: luma {li:.1} -> {lo:.1}"
        );
        // Progresivo: al 25 % hay menos tinte que al 5 %, pero ya se nota.
        let mid = rgb(perceive(&source, 0.25, 1.3, FOCUS, false).unwrap()[at(w, h, FOCUS)]);
        assert!(
            violet(mid) > violet(input) && violet(mid) < violet(critical),
            "{mid:?}"
        );
    }
    // El negro sigue negro y el fondo espacial no se rellena de morado.
    for void in [0x000000, 0x03040a] {
        let source = Framebuffer::new(w, h, void).unwrap();
        let out = rgb(perceive(&source, 0.05, 1.3, FOCUS, false).unwrap()[at(w, h, FOCUS)]);
        assert!(out.iter().all(|&c| c <= 12), "{void:06x} -> {out:?}");
    }
}

#[test]
fn the_tunnel_closes_around_the_projected_ship_not_the_screen_center() {
    let (w, h) = (320, 180);
    let source = Framebuffer::new(w, h, 0x8a8a8a).unwrap();
    let black = |o: &[u32]| o.iter().filter(|&&p| p == 0).count();
    let mut previous_black = 0;
    for oxygen in [0.25, 0.1, 0.05, 0.01] {
        let output = perceive(&source, oxygen, 1.3, FOCUS, false).unwrap();
        // La nave (el foco) sigue visible: al 5 % y al 1 % no hay blackout.
        let focus = rgb(output[at(w, h, FOCUS)]);
        assert!(luma(focus) > 60.0, "O2 {oxygen}: foco {focus:?}");
        // Simetria respecto del FOCO (no del centro de la pantalla).
        let (fx, fy) = (
            (FOCUS[0] * w as f64) as usize,
            (FOCUS[1] * h as f64) as usize,
        );
        let (left, right) = (output[fy * w + fx - 30], output[fy * w + fx + 30]);
        assert!(
            (luma(rgb(left)) - luma(rgb(right))).abs() <= 2.0,
            "O2 {oxygen}"
        );
        let count = black(&output);
        assert!(count >= previous_black, "O2 {oxygen}: el tunel se reabre");
        previous_black = count;
    }
    let critical = perceive(&source, 0.05, 1.3, FOCUS, false).unwrap();
    // Casi solo la nave: el centro geometrico y las esquinas ya son negro.
    for uv in [
        [0.5, 0.5],
        [0.0, 0.0],
        [0.99, 0.0],
        [0.99, 0.99],
        [0.0, 0.99],
    ] {
        assert_eq!(critical[at(w, h, uv)], 0, "{uv:?} deberia ser negro");
    }
    assert!(black(&critical) * 10 > w * h * 8, "menos del 80 % negro");
    // Cerca de la nave, una zona legible y no un punto.
    let (fx, fy) = (
        (FOCUS[0] * w as f64) as usize,
        (FOCUS[1] * h as f64) as usize,
    );
    for (dx, dy) in [(-15i64, 0i64), (15, 0), (0, -15), (0, 15)] {
        let i = (fy as i64 + dy) as usize * w + (fx as i64 + dx) as usize;
        assert!(luma(rgb(critical[i])) > 60.0, "({dx},{dy}) cerca del foco");
    }
}

/// Franjas verticales blanco/negro de 6 px: solo dos colores en la fuente.
fn stripes(width: usize, height: usize) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, 0).unwrap();
    for y in 0..height {
        for x in 0..width {
            fb.pixels[y * width + x] = if (x / 6) % 2 == 0 { 0xffffff } else { 0 };
        }
    }
    fb
}

/// Fila del foco, +-20 px: dentro de la zona del tunel plenamente visible.
fn focus_row(output: &[u32], w: usize, h: usize) -> Vec<[i32; 3]> {
    let (fx, fy) = (
        (FOCUS[0] * w as f64) as usize,
        (FOCUS[1] * h as f64) as usize,
    );
    (fx - 20..fx + 20)
        .map(|x| rgb(output[fy * w + x]))
        .collect()
}

fn largest_step(row: &[[i32; 3]]) -> f64 {
    row.windows(2)
        .map(|p| (luma(p[0]) - luma(p[1])).abs())
        .fold(0.0, f64::max)
}

#[test]
fn vision_blurs_by_filtering_neighbours_independently_of_tint_and_tunnel() {
    let (w, h) = (640, 360);
    let source = stripes(w, h);
    // Contraste que dejaria el tinte solo (sobre imagenes uniformes no hay desenfoque visible).
    let white = Framebuffer::new(w, h, 0xffffff).unwrap();
    let tinted_white = luma(rgb(
        perceive(&white, 0.05, 1.3, FOCUS, false).unwrap()[at(w, h, FOCUS)]
    ));
    let critical = focus_row(&perceive(&source, 0.05, 1.3, FOCUS, false).unwrap(), w, h);
    let mut colors = critical.clone();
    colors.sort();
    colors.dedup();
    // Un filtro por pixel (tinte) solo puede dar dos colores; el desenfoque crea intermedios.
    assert!(
        colors.len() >= 6,
        "solo {} colores: {colors:?}",
        colors.len()
    );
    assert!(
        largest_step(&critical) < 0.5 * tinted_white,
        "bordes sin suavizar"
    );
    // Progresivo: al 25 % hay desenfoque, pero menos que en el tramo critico.
    let mid = focus_row(&perceive(&source, 0.25, 1.3, FOCUS, false).unwrap(), w, h);
    assert!(largest_step(&mid) > largest_step(&critical));
}

#[test]
fn the_final_blackout_arrives_continuously_while_one_percent_still_shows_the_ship() {
    let (w, h) = (320, 180);
    let grey = Framebuffer::new(w, h, 0x8a8a8a).unwrap();
    let focus_luma = |o: f64| {
        luma(rgb(
            perceive(&grey, o, 1.3, FOCUS, false).unwrap()[at(w, h, FOCUS)]
        ))
    };
    assert!(focus_luma(0.01) > 60.0, "al 1 % la nave aun se ve");
    let mut previous = focus_luma(0.02);
    for step in 1..=20 {
        let oxygen = 0.02 - step as f64 * 0.001;
        let now = focus_luma(oxygen.max(0.0));
        assert!(now <= previous + 1e-9, "O2 {oxygen}: se aclara al bajar");
        previous = now;
    }
    // Sin salto en el ultimo tramo: rozando 0 ya casi es negro.
    let nearly = perceive(&grey, 1e-6, 1.3, FOCUS, false).unwrap();
    assert!(max_delta(&nearly, &vec![0; w * h]) <= 1);
    // Pasos finos en todo el rango: nunca un salto de umbral.
    let source = varied(w, h);
    for oxygen in [
        0.5, 0.4, 0.3, 0.2, 0.1, 0.05, 0.02, 0.009, 0.005, 0.001, 2e-5,
    ] {
        let a = perceive(&source, oxygen, 1.3, FOCUS, false).unwrap();
        let b = perceive(&source, oxygen - 1e-5, 1.3, FOCUS, false).unwrap();
        assert!(max_delta(&a, &b) <= 3, "salto en O2 {oxygen}");
    }
}

/// Las cuatro rutas: identidad, negro por O2 0, negro por ataque y efecto critico.
const PATHS: [(f64, bool); 4] = [(1.0, false), (0.0, false), (0.7, true), (0.05, false)];

#[test]
fn invalid_oxygen_time_or_focus_fail_clearly_even_on_shortcuts() {
    let source = varied(64, 36);
    for (oxygen, attack) in PATHS {
        let bad_oxygen = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01];
        for o in bad_oxygen {
            let error = perceive(&source, o, 1.0, FOCUS, attack).expect_err("O2 invalido aceptado");
            assert!(error.contains("O2"), "{error}");
        }
        for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.5] {
            let error =
                perceive(&source, oxygen, t, FOCUS, attack).expect_err("tiempo invalido aceptado");
            assert!(error.contains("tiempo"), "{error}");
        }
        for v in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
            for focus in [[v, 0.5], [0.5, v]] {
                let error = perceive(&source, oxygen, 1.0, focus, attack)
                    .expect_err("foco invalido aceptado");
                assert!(error.contains("foco"), "{error}");
            }
        }
        // Extremos validos.
        for focus in [[0.0, 0.0], [1.0, 1.0]] {
            assert!(perceive(&source, oxygen, 0.0, focus, attack).is_ok());
        }
        assert!(perceive(&source, oxygen, 1.0e15, FOCUS, attack).is_ok());
    }
}

#[test]
fn incongruent_or_oversized_framebuffers_fail_before_reserving() {
    for (oxygen, attack) in PATHS {
        let mut source = varied(2, 2);
        source.pixels.extend([0x112233; 4]);
        assert!(
            perceive(&source, oxygen, 1.0, FOCUS, attack).is_err(),
            "pixels de mas"
        );
        let mut source = varied(16, 9);
        source.pixels.truncate(100);
        assert!(
            perceive(&source, oxygen, 1.0, FOCUS, attack).is_err(),
            "pixels de menos"
        );
        for (w, h) in [
            (usize::MAX, 2),
            (2, usize::MAX),
            (0, 16),
            (16, 0),
            (4097, 4096),
        ] {
            let mut source = varied(4, 4);
            (source.width, source.height) = (w, h);
            let error = perceive(&source, oxygen, 1.0, FOCUS, attack).expect_err("dimensiones");
            assert!(error.contains("framebuffer"), "{w}x{h}: {error}");
        }
    }
}

// --- Guards nacidos GREEN (sensibilidad probada con mutaciones en copia con otro target) ---

#[test]
fn same_inputs_give_the_same_frame() {
    let (a, b) = (varied(320, 180), varied(320, 180));
    for oxygen in [0.3, 0.05, 0.01, 0.004] {
        let first = perceive(&a, oxygen, 2.0, FOCUS, false).unwrap();
        assert_eq!(first, perceive(&a, oxygen, 2.0, FOCUS, false).unwrap());
        assert_eq!(first, perceive(&b, oxygen, 2.0, FOCUS, false).unwrap());
    }
}

#[test]
fn the_effect_is_static_in_time_so_there_are_no_flashes_or_pulses() {
    let source = varied(320, 180);
    for oxygen in [0.3, 0.05, 0.01] {
        let first = perceive(&source, oxygen, 0.0, FOCUS, false).unwrap();
        for step in 1..25 {
            let time = step as f64 * 0.173;
            assert_eq!(
                first,
                perceive(&source, oxygen, time, FOCUS, false).unwrap(),
                "t {time}"
            );
        }
        assert_eq!(
            first,
            perceive(&source, oxygen, 1.0e15, FOCUS, false).unwrap()
        );
    }
}

#[test]
fn at_one_percent_the_whole_ship_area_stays_visible_at_game_resolution() {
    // A 1280x720 la nave del arranque ocupa ~120 px alrededor de su punto proyectado.
    let (w, h) = (1280, 720);
    let focus = [0.5, 455.0 / 720.0];
    let source = Framebuffer::new(w, h, 0x8a8a8a).unwrap();
    let output = perceive(&source, 0.01, 1.3, focus, false).unwrap();
    let reference = luma(rgb(output[at(w, h, focus)]));
    assert!(reference > 60.0);
    let (fx, fy) = (focus[0] * w as f64, focus[1] * h as f64);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f64 + 0.5 - fx).powi(2) + (y as f64 + 0.5 - fy).powi(2)).sqrt();
            if d <= 0.17 * h as f64 {
                let l = luma(rgb(output[y * w + x]));
                assert!(l >= 0.9 * reference, "({x},{y}) a {d:.0} px: {l:.1}");
            }
        }
    }
}

#[test]
fn degenerate_and_non_square_sizes_stay_in_bounds() {
    for (w, h) in [
        (1, 1),
        (1, 9),
        (9, 1),
        (1, 300),
        (300, 1),
        (7, 3),
        (3, 7),
        (1280, 720),
    ] {
        let source = varied(w, h);
        for oxygen in [0.25, 0.05, 0.01, 0.003] {
            for focus in [[0.0, 0.0], [1.0, 1.0], FOCUS] {
                let output = perceive(&source, oxygen, 0.7, focus, false).unwrap();
                assert_eq!(output.len(), w * h, "{w}x{h}");
            }
        }
    }
}

#[test]
fn the_rendered_source_frame_and_depth_are_left_untouched() {
    let mut source = varied(320, 180);
    source.set_color(0x445566);
    for x in 0..320 {
        assert!(source.point(x, 120, 0.5));
    }
    let (pixels, width, height) = (source.pixels.clone(), source.width, source.height);
    for (oxygen, attack) in PATHS {
        let _ = perceive(&source, oxygen, 2.0, FOCUS, attack).unwrap();
    }
    assert_ne!(perceive(&source, 0.05, 2.0, FOCUS, false).unwrap(), pixels);
    assert_eq!(
        (&source.pixels, source.width, source.height),
        (&pixels, width, height)
    );
    for x in 0..320 {
        assert!(!source.depth_passes(x, 120, 0.5) && source.depth_passes(x, 120, 0.4));
        assert!(source.depth_passes(x, 121, 1.0e9));
    }
}

// Supervisor regression: RGB quantization must not masquerade as the zero-O2 trigger.
#[test]
fn positive_oxygen_does_not_quantize_the_entire_visible_ship_to_black() {
    let source = Framebuffer::new(320, 180, 0x8090a0).unwrap();
    for oxygen in [0.001, 0.0001] {
        let output = perceive(&source, oxygen, 1.3, FOCUS, false).unwrap();
        assert!(
            output.iter().any(|&p| p != 0),
            "O2 {oxygen} is positive without attack: full-frame blackout is premature"
        );
    }
}
