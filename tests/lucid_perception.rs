//! Supervisor tracer: a bounded peripheral effect, not stars or a gameplay integration.
#[path = "../src/visual/lucid_perception.rs"]
mod lucid_perception;

use umbral::renderer::framebuffer::Framebuffer;

#[test]
fn critical_oxygen_darkens_the_periphery_while_the_center_remains_readable() {
    let source = Framebuffer::new(320, 180, 0x8090a0).unwrap();
    let output = lucid_perception::lucid_perception(&source, 0.05, 1.3).unwrap();
    assert_eq!(output.len(), source.pixels.len());
    assert!(
        output.iter().zip(&source.pixels).any(|(a, b)| a != b),
        "critical oxygen must produce a visible peripheral change, not the identity placeholder"
    );
    assert_eq!(output[90 * 320 + 160], source.pixels[90 * 320 + 160]);
    assert_eq!(source.pixels, vec![0x8090a0; 320 * 180]);
}

// --- Bronya: mecanismos de la hipotesis (cada bloque nacio con su RED; ver worker-evidence) ---

use lucid_perception::{PERCEPTION_FULL_OXYGEN, PERCEPTION_ONSET_OXYGEN, lucid_perception};

/// Imagen con color distinto en cada pixel (no uniforme), para que cualquier cambio se note.
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

fn channels(p: u32) -> [i32; 3] {
    [
        (p >> 16) as i32 & 0xff,
        (p >> 8) as i32 & 0xff,
        p as i32 & 0xff,
    ]
}

/// Mayor diferencia por canal entre dos imagenes.
fn max_delta(a: &[u32], b: &[u32]) -> i32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .flat_map(|(&x, &y)| {
            let (x, y) = (channels(x), channels(y));
            (0..3).map(move |i| (x[i] - y[i]).abs())
        })
        .max()
        .unwrap_or(0)
}

fn luminance(p: u32) -> i32 {
    channels(p).iter().sum()
}

#[test]
fn high_oxygen_keeps_every_pixel_exact_at_any_time() {
    let source = varied(320, 180);
    for oxygen in [1.0, 0.9, 0.75, 0.6, PERCEPTION_ONSET_OXYGEN] {
        for time in [0.0, 1.3, 7.25, 1.0e6] {
            let output = lucid_perception(&source, oxygen, time).unwrap();
            assert_eq!(
                output, source.pixels,
                "O2 {oxygen} t {time} debe ser identidad"
            );
        }
    }
}

#[test]
fn peripheral_darkening_grows_continuously_without_threshold_jumps() {
    let source = Framebuffer::new(320, 180, 0x808080).unwrap();
    let corner = 0;
    // Justo bajo el umbral: a lo sumo un nivel de redondeo, nunca un salto.
    let just_below = lucid_perception(&source, PERCEPTION_ONSET_OXYGEN - 1e-4, 2.0).unwrap();
    assert!(max_delta(&just_below, &source.pixels) <= 1);
    let mut previous = source.pixels.clone();
    let mut oxygen = PERCEPTION_ONSET_OXYGEN;
    while oxygen >= 0.0 {
        let output = lucid_perception(&source, oxygen, 2.0).unwrap();
        // Al bajar el O2 la periferia nunca se aclara y cada paso de 1 % cambia poco.
        assert!(
            luminance(output[corner]) <= luminance(previous[corner]),
            "O2 {oxygen}"
        );
        assert!(max_delta(&output, &previous) <= 6, "salto en O2 {oxygen}");
        previous = output;
        oxygen -= 0.01;
    }
    let critical = lucid_perception(&source, PERCEPTION_FULL_OXYGEN, 2.0).unwrap();
    assert!(luminance(critical[corner]) * 2 < luminance(source.pixels[corner]));
}

#[test]
fn invalid_oxygen_or_time_is_a_clear_error_even_with_high_oxygen() {
    let source = varied(64, 36);
    for oxygen in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01, 2.0] {
        let error = lucid_perception(&source, oxygen, 1.0).expect_err("O2 invalido aceptado");
        assert!(error.contains("O2"), "{error}");
    }
    // El tiempo se valida antes del atajo de identidad: O2 alto no esconde el error.
    for oxygen in [1.0, 0.05] {
        for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.5] {
            let error =
                lucid_perception(&source, oxygen, time).expect_err("tiempo invalido aceptado");
            assert!(error.contains("tiempo"), "{error}");
        }
    }
    // Los extremos validos se aceptan.
    assert!(lucid_perception(&source, 0.0, 0.0).is_ok());
    assert!(lucid_perception(&source, 1.0, 1.0e12).is_ok());
}

#[test]
fn publicly_mutated_dimensions_fail_before_reserving_or_writing() {
    for oxygen in [1.0, 0.05] {
        // Mas pixeles que width*height: no se devuelve un cuadro incongruente.
        let mut source = varied(2, 2);
        source.pixels.extend([0x112233; 4]);
        assert!(
            lucid_perception(&source, oxygen, 1.0).is_err(),
            "pixels de mas"
        );
        // Menos pixeles que width*height.
        let mut source = varied(16, 9);
        source.pixels.truncate(100);
        assert!(
            lucid_perception(&source, oxygen, 1.0).is_err(),
            "pixels de menos"
        );
        // Overflow de width*height y dimensiones nulas.
        for (width, height) in [(usize::MAX, 2), (2, usize::MAX), (usize::MAX, usize::MAX)] {
            let mut source = varied(4, 4);
            (source.width, source.height) = (width, height);
            assert!(
                lucid_perception(&source, oxygen, 1.0).is_err(),
                "{width}x{height}"
            );
        }
        for (width, height) in [(0, 16), (16, 0), (0, 0)] {
            let mut source = varied(4, 4);
            (source.width, source.height) = (width, height);
            assert!(
                lucid_perception(&source, oxygen, 1.0).is_err(),
                "{width}x{height}"
            );
        }
        // Por encima de MAX_PIXELS aunque el producto no desborde.
        let mut source = varied(4, 4);
        (source.width, source.height) = (4097, 4096);
        assert!(lucid_perception(&source, oxygen, 1.0).is_err(), "4097x4096");
    }
}

/// Pixeles dentro de la elipse protegida (radio normalizado a la esquina < CLEAR_RADIUS).
fn inside_clear_area(width: usize, height: usize, x: usize, y: usize) -> bool {
    let (hw, hh) = (width as f64 / 2.0, height as f64 / 2.0);
    let dx = (x as f64 + 0.5 - hw) / hw;
    let dy = (y as f64 + 0.5 - hh) / hh;
    ((dx * dx + dy * dy) / 2.0).sqrt() < lucid_perception::CLEAR_RADIUS
}

#[test]
fn only_critical_oxygen_deforms_the_periphery_over_simulated_time() {
    use lucid_perception::{DEFORM_ONSET_OXYGEN, DEFORM_PERIOD_SECONDS};
    let source = varied(320, 180);
    let quarter = DEFORM_PERIOD_SECONDS / 4.0;
    // Advertencia (bajo el umbral de oscurecimiento, sobre el critico): oscurece, no se mueve.
    let warning = (PERCEPTION_ONSET_OXYGEN + DEFORM_ONSET_OXYGEN) / 2.0;
    let a = lucid_perception(&source, warning, 1.0).unwrap();
    assert_ne!(a, source.pixels);
    assert_eq!(
        a,
        lucid_perception(&source, warning, 1.0 + quarter).unwrap()
    );
    // Critico: la periferia cambia con el tiempo simulado.
    for oxygen in [PERCEPTION_FULL_OXYGEN, 0.0] {
        let a = lucid_perception(&source, oxygen, 1.0).unwrap();
        let b = lucid_perception(&source, oxygen, 1.0 + quarter).unwrap();
        let moved = a.iter().zip(&b).filter(|(x, y)| x != y).count();
        assert!(
            moved * 20 > a.len(),
            "O2 {oxygen}: solo {moved} pixeles cambian con el tiempo"
        );
        // ...pero el area central protegida sigue exacta en ambos instantes.
        for y in 0..180 {
            for x in 0..320 {
                if inside_clear_area(320, 180, x, y) {
                    let i = y * 320 + x;
                    assert_eq!(
                        (a[i], b[i]),
                        (source.pixels[i], source.pixels[i]),
                        "({x},{y})"
                    );
                }
            }
        }
    }
}

// --- Guards nacidos GREEN (su sensibilidad se probo con mutaciones en una copia desechable) ---

/// Rampa suave: R codifica x, G codifica y, B constante (sin saltos de alta frecuencia).
fn smooth(width: usize, height: usize) -> Framebuffer {
    assert!(width <= 256 && height <= 256);
    let mut fb = Framebuffer::new(width, height, 0).unwrap();
    for y in 0..height {
        for x in 0..width {
            fb.pixels[y * width + x] = ((x as u32) << 16) | ((y as u32) << 8) | 0xff;
        }
    }
    fb
}

#[test]
fn deformation_samples_the_source_within_a_small_bounded_radius() {
    use lucid_perception::DEFORM_MAX_FRACTION;
    let (width, height) = (256, 220);
    let source = smooth(width, height);
    let limit = DEFORM_MAX_FRACTION * height as f64;
    let mut largest: f64 = 0.0;
    for time in [0.0, 0.4, 0.9, 1.7, 2.2] {
        let output = lucid_perception(&source, 0.0, time).unwrap();
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = channels(output[y * width + x]);
                // B (255 en la fuente) da el factor de oscurecimiento; con el se recupera de que
                // coordenada de la fuente salio el color.
                let keep = b as f64 / 255.0;
                if keep < 0.4 {
                    continue; // demasiado oscuro para recuperar la coordenada con precision
                }
                let (sx, sy) = (r as f64 / keep, g as f64 / keep);
                let distance = ((sx - x as f64).powi(2) + (sy - y as f64).powi(2)).sqrt();
                // Error de medida por la cuantizacion a 8 bits de R/G (0.5) y de B (0.5/255).
                let error = |c: f64| (c * 0.5 / 255.0 + 0.5) / keep;
                let tolerance = error(sx).hypot(error(sy)) + 1e-6;
                assert!(
                    distance <= limit + tolerance,
                    "({x},{y}) t {time}: {distance:.2} px > {limit} + {tolerance:.2}"
                );
                largest = largest.max(distance);
            }
        }
    }
    assert!(
        largest >= 1.5,
        "la deformacion es real, no solo redondeo: {largest:.2} px"
    );
}

#[test]
fn same_inputs_give_the_same_frame() {
    let source = varied(320, 180);
    let copy = varied(320, 180);
    for (oxygen, time) in [(0.05, 1.3), (0.0, 4.75), (0.2, 0.0), (0.4, 9.0)] {
        let a = lucid_perception(&source, oxygen, time).unwrap();
        assert_eq!(a, lucid_perception(&source, oxygen, time).unwrap());
        assert_eq!(a, lucid_perception(&copy, oxygen, time).unwrap());
    }
}

#[test]
fn darkening_is_static_in_time_so_there_is_no_flicker() {
    // En una imagen uniforme la deformacion es invisible: solo queda el oscurecimiento, que no
    // debe latir, parpadear ni cambiar con el tiempo.
    let source = Framebuffer::new(320, 180, 0x6a7a8a).unwrap();
    for oxygen in [0.25, PERCEPTION_FULL_OXYGEN, 0.0] {
        let first = lucid_perception(&source, oxygen, 0.0).unwrap();
        for step in 1..40 {
            let time = step as f64 * 0.137;
            assert_eq!(
                first,
                lucid_perception(&source, oxygen, time).unwrap(),
                "t {time}"
            );
        }
    }
}

/// Ondas suaves de alto contraste (hasta ~33 niveles por pixel): cualquier salto de la
/// deformacion se nota, un movimiento continuo apenas.
fn waves(width: usize, height: usize) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, 0).unwrap();
    let wave = |v: usize| (127.5 + 127.0 * (v as f64 * std::f64::consts::TAU / 24.0).sin()) as u32;
    for y in 0..height {
        for x in 0..width {
            fb.pixels[y * width + x] = (wave(x) << 16) | (wave(y) << 8) | 0x80;
        }
    }
    fb
}

#[test]
fn deformation_moves_smoothly_in_time_without_jitter() {
    let source = waves(256, 180);
    // Varios pasos inconmensurables: un salto de fase periodico no puede esconderse por aliasing
    // en una sola rejilla temporal.
    for oxygen in [PERCEPTION_FULL_OXYGEN, 0.0] {
        for dt in [0.01, 0.0037, 0.0011] {
            let mut previous = lucid_perception(&source, oxygen, 3.0).unwrap();
            for step in 1..=50 {
                let time = 3.0 + step as f64 * dt;
                let output = lucid_perception(&source, oxygen, time).unwrap();
                assert!(max_delta(&output, &previous) <= 2, "t {time} dt {dt}");
                previous = output;
            }
        }
    }
}

#[test]
fn deformation_starts_continuously_at_its_critical_threshold() {
    use lucid_perception::DEFORM_ONSET_OXYGEN;
    let source = smooth(256, 180);
    let at = lucid_perception(&source, DEFORM_ONSET_OXYGEN, 1.0).unwrap();
    let below = lucid_perception(&source, DEFORM_ONSET_OXYGEN - 1e-4, 1.0).unwrap();
    assert!(max_delta(&at, &below) <= 1);
    let mut previous = at;
    let mut oxygen = DEFORM_ONSET_OXYGEN;
    while oxygen >= 0.0 {
        let output = lucid_perception(&source, oxygen, 1.0).unwrap();
        assert!(max_delta(&output, &previous) <= 6, "salto en O2 {oxygen}");
        previous = output;
        oxygen -= 0.01;
    }
}

#[test]
fn degenerate_and_non_square_sizes_stay_in_bounds() {
    for (width, height) in [
        (1, 1),
        (1, 9),
        (9, 1),
        (1, 300),
        (300, 1),
        (7, 3),
        (3, 7),
        (1280, 720),
    ] {
        let source = varied(width, height);
        for oxygen in [0.0, 0.05, 0.25] {
            for time in [0.0, 0.6, 1.9] {
                let output = lucid_perception(&source, oxygen, time).unwrap();
                assert_eq!(output.len(), width * height, "{width}x{height}");
                if width * height == 1 {
                    assert_eq!(output, source.pixels, "1x1 es el centro: intacto");
                }
            }
        }
    }
}

#[test]
fn the_rendered_source_frame_is_left_untouched() {
    let mut source = varied(320, 180);
    // Un z-buffer no trivial: algunos pixeles con profundidad escrita.
    source.set_color(0x445566);
    for x in 0..320 {
        assert!(source.point(x, 20, 0.5));
    }
    let (pixels, width, height) = (source.pixels.clone(), source.width, source.height);
    let output = lucid_perception(&source, 0.0, 2.0).unwrap();
    assert_ne!(output, pixels);
    assert_eq!(
        (&source.pixels, source.width, source.height),
        (&pixels, width, height)
    );
    for x in 0..320 {
        assert!(!source.depth_passes(x, 20, 0.5) && source.depth_passes(x, 20, 0.4));
        assert!(source.depth_passes(x, 21, 1.0e9));
    }
}

#[test]
fn a_central_box_stays_readable_whatever_the_tuning_constants_say() {
    // Contrato independiente de `CLEAR_RADIUS`: el 30 % central (mira, nave y composicion) no
    // se oscurece ni se deforma, ni siquiera sin oxigeno.
    for (width, height) in [(320, 180), (1280, 720), (200, 300)] {
        let source = varied(width, height);
        for oxygen in [0.0, PERCEPTION_FULL_OXYGEN, 0.2] {
            for time in [0.0, 0.8, 1.9] {
                let output = lucid_perception(&source, oxygen, time).unwrap();
                for y in height * 35 / 100..height * 65 / 100 {
                    for x in width * 35 / 100..width * 65 / 100 {
                        let i = y * width + x;
                        assert_eq!(output[i], source.pixels[i], "{width}x{height} ({x},{y})");
                    }
                }
            }
        }
    }
}
