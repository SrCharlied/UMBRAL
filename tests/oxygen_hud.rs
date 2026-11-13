//! HUD de reserva de oxigeno (hud_oxygen_01): lectura, layout y dibujo del instrumento hundido en el borde
//! superior. Pruebas conductuales sobre el framebuffer real; no certifican aprobacion artistica.

use umbral::{
    renderer::framebuffer::Framebuffer,
    visual::oxygen_hud::{
        HUD_FILL, HUD_TEXT, HUD_TRACK, OxygenHudLayout, OxygenReading, draw_oxygen_hud,
        oxygen_hud_layout,
    },
};

const BG: u32 = 0x20_30_40;

/// Muestra del arco: pixel `(x, y)` y su angulo.
type Sample = (usize, usize, f64);

fn frame(w: usize, h: usize) -> Framebuffer {
    Framebuffer::new(w, h, BG).unwrap()
}

fn reading(v: f64) -> OxygenReading {
    OxygenReading::new(v).unwrap()
}

fn dist2(a: u32, b: u32) -> i64 {
    (0..3)
        .map(|i| {
            let d = ((a >> (8 * i)) & 0xff) as i64 - ((b >> (8 * i)) & 0xff) as i64;
            d * d
        })
        .sum()
}

/// Pixeles cuyo CENTRO cae en el radio medio del arco de reserva, lejos de sus extremos y de las marcas,
/// con su angulo (0 = abajo, negativo = izquierda).
fn centreline(l: &OxygenHudLayout) -> Vec<Sample> {
    let mid = (l.track[0] + l.track[1]) / 2.0;
    let mut out = Vec::new();
    for y in l.bounds.y0..l.bounds.y1 {
        for x in l.bounds.x0..l.bounds.x1 {
            let (dx, dy) = (x as f64 + 0.5 - l.center[0], y as f64 + 0.5 - l.center[1]);
            let r = dx.hypot(dy);
            let phi = dx.atan2(dy);
            let near_tick = [-0.5, 0.0, 0.5]
                .iter()
                .any(|t| ((phi - t * l.half_span).abs() * r) < 1.5 * l.scale as f64);
            if (r - mid).abs() <= 0.5
                && phi.abs() <= l.half_span - 1.5 * l.scale as f64 / r
                && !near_tick
            {
                out.push((x, y, phi));
            }
        }
    }
    assert!(
        out.len() >= 20,
        "muy pocas muestras del arco: {}",
        out.len()
    );
    out
}

fn filled(fb: &Framebuffer, (x, y, _): Sample) -> bool {
    let p = fb.pixels[y * fb.width + x];
    dist2(p, HUD_FILL) < dist2(p, HUD_TRACK)
}

#[test]
fn hud01_reading_rejects_non_finite_and_out_of_range() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        assert!(OxygenReading::new(v).is_err(), "acepto {v}");
    }
    assert_eq!(reading(0.5).fraction(), 0.5);
}

#[test]
fn hud02_percent_is_coherent_with_the_reserve() {
    for (v, p) in [
        (0.0, 0),
        (1.0, 100),
        (0.72, 72),
        (0.25, 25),
        (0.5, 50),
        (0.004, 1),
        (0.996, 99),
    ] {
        assert_eq!(reading(v).percent(), p, "reserva {v}");
    }
    assert_eq!(reading(0.72).label(), "O2 72%");
    assert_eq!(reading(0.0).label(), "O2 0%");
}

#[test]
fn hud03_layout_at_reference_resolutions_is_compact_and_centered() {
    for (w, h, scale, max_h_pct, max_w_pct) in [(320, 180, 1, 15, 28), (1280, 720, 2, 8, 14)] {
        let l = oxygen_hud_layout(w, h).unwrap_or_else(|| panic!("sin layout {w}x{h}"));
        assert_eq!(l.scale, scale, "{w}x{h}");
        assert_eq!(l.bounds.y0, 0, "el disco nace del borde superior");
        assert!(l.center[1] < 0.0, "centro del disco por ENCIMA del borde");
        assert!(l.bounds.x1 <= w && l.bounds.y1 <= h);
        assert_eq!(l.bounds.x0 + l.bounds.x1, w, "centrado horizontal");
        assert!(l.bounds.height() * 100 <= h * max_h_pct, "{w}x{h}: {l:?}");
        assert!(l.bounds.width() * 100 <= w * max_w_pct, "{w}x{h}: {l:?}");
        let e = l.eye_reserve;
        assert!(e.y0 > l.bounds.y1 && e.y1 <= h && e.x1 <= w && e.width() > 0 && e.height() > 0);
        assert_eq!(
            e.x0 + e.x1,
            w,
            "reserva del ojo centrada bajo el instrumento"
        );
    }
}

#[test]
fn hud04_tiny_and_narrow_frames_never_panic_or_leave_the_frame() {
    assert!(oxygen_hud_layout(1, 1).is_none());
    assert!(oxygen_hud_layout(40, 20).is_none());
    let sizes = [
        1usize, 2, 3, 8, 16, 40, 60, 64, 80, 90, 100, 120, 160, 200, 240, 320, 333, 480, 641, 800,
        1024, 1280, 1920, 2560, 4096,
    ];
    let mut some = 0;
    for &w in &sizes {
        for &h in &sizes {
            if w * h > 4096 * 4096 || w * h > 2_500_000 {
                continue;
            }
            let mut fb = frame(w, h);
            let before = fb.pixels.clone();
            draw_oxygen_hud(&mut fb, reading(0.72));
            match oxygen_hud_layout(w, h) {
                None => assert!(fb.pixels == before, "{w}x{h}: dibujo sin layout"),
                Some(l) => {
                    some += 1;
                    assert!(l.bounds.x1 <= w && l.bounds.y1 <= h, "{w}x{h}");
                    assert!(l.eye_reserve.x1 <= w && l.eye_reserve.y1 <= h, "{w}x{h}");
                    for y in 0..h {
                        for x in 0..w {
                            if !l.bounds.contains(x, y) {
                                assert_eq!(fb.pixels[y * w + x], before[y * w + x], "{w}x{h}");
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(some > 50, "casi ningun tamano tuvo HUD: {some}");
    // Ventana estrecha y alta.
    assert!(oxygen_hud_layout(100, 400).is_some());
}

#[test]
fn hud05_draw_touches_only_its_bounds_never_depth_nor_the_eye_reserve() {
    let mut fb = frame(320, 180);
    let l = oxygen_hud_layout(320, 180).unwrap();
    let probes = [
        (160usize, 2usize),
        (l.bounds.x0 + 2, 1),
        (160, l.bounds.y1 - 1),
        (160, 100),
    ];
    for &(x, y) in &probes {
        assert!(fb.write(x as isize, y as isize, 5.0, 0x777777));
    }
    let before = fb.pixels.clone();
    draw_oxygen_hud(&mut fb, reading(0.72));
    assert!(fb.pixels != before, "precondicion: el HUD dibuja");
    for y in 0..180 {
        for x in 0..320 {
            if !l.bounds.contains(x, y) || l.eye_reserve.contains(x, y) {
                assert_eq!(fb.pixels[y * 320 + x], before[y * 320 + x], "({x},{y})");
            }
        }
    }
    for &(x, y) in &probes {
        assert!(
            fb.depth_passes(x as isize, y as isize, 4.9),
            "profundidad ({x},{y})"
        );
        assert!(
            !fb.depth_passes(x as isize, y as isize, 5.1),
            "profundidad ({x},{y})"
        );
    }
}

#[test]
fn hud06_arc_full_empty_monotonic_and_unidirectional() {
    for (w, h) in [(320usize, 180usize), (1280, 720)] {
        let l = oxygen_hud_layout(w, h).unwrap();
        let samples = centreline(&l);
        let draw = |v: f64| {
            let mut fb = frame(w, h);
            draw_oxygen_hud(&mut fb, reading(v));
            fb
        };
        let full = draw(1.0);
        assert!(samples.iter().all(|&s| filled(&full, s)), "{w}x{h}: lleno");
        let empty = draw(0.0);
        assert!(
            samples.iter().all(|&s| !filled(&empty, s)),
            "{w}x{h}: vacio"
        );
        let mut last = 0;
        for i in 0..=100 {
            let v = i as f64 / 100.0;
            let fb = draw(v);
            let (on, off): (Vec<Sample>, Vec<Sample>) =
                samples.iter().copied().partition(|&s| filled(&fb, s));
            assert!(on.len() >= last, "{w}x{h}: no monotono en {v}");
            last = on.len();
            // Un solo sentido: lo lleno es siempre el tramo IZQUIERDO; se vacia de derecha a izquierda.
            let max_on = on.iter().map(|s| s.2).fold(f64::NEG_INFINITY, f64::max);
            let min_off = off.iter().map(|s| s.2).fold(f64::INFINITY, f64::min);
            assert!(max_on < min_off, "{w}x{h}: no unidireccional en {v}");
            // Arco y numero: el mismo dato. El borde lleno/vacio cae en el angulo de la lectura.
            let end = l.fill_end_angle(reading(v));
            let mid = (l.track[0] + l.track[1]) / 2.0;
            for s in &on {
                assert!(
                    s.2 <= end + 1.0 / mid,
                    "{w}x{h}: lleno pasado el final en {v}"
                );
            }
            for s in &off {
                assert!(
                    s.2 >= end - 1.0 / mid,
                    "{w}x{h}: vacio antes del final en {v}"
                );
            }
        }
        assert!(
            (l.fill_end_angle(reading(0.0)) + l.half_span).abs() < 1e-12
                && (l.fill_end_angle(reading(1.0)) - l.half_span).abs() < 1e-12
        );
    }
}

#[test]
fn hud07_label_inside_the_disc_clear_of_the_arc_and_drawn() {
    for (w, h) in [(320usize, 180usize), (1280, 720), (100, 400)] {
        let l = oxygen_hud_layout(w, h).unwrap();
        let k = l.scale as f64;
        for text in ["O2 100%", "O2 72%", "O2 0%"] {
            let tw = (text.len() * 6 - 1) as f64 * k;
            let [ox, oy] = l.label_origin(text);
            let x0 = ox as f64;
            assert!(
                (x0 - (w as f64 - tw) / 2.0).abs() <= 1.0,
                "{w}x{h} {text}: no centrado"
            );
            let y0 = oy as f64;
            for (cx, cy) in [
                (x0, y0),
                (x0 + tw, y0),
                (x0, y0 + 7.0 * k),
                (x0 + tw, y0 + 7.0 * k),
            ] {
                let (dx, dy) = (cx - l.center[0], cy - l.center[1]);
                let r = dx.hypot(dy);
                let limit = if dx.atan2(dy).abs() <= l.half_span {
                    l.track[0] - 0.5
                } else {
                    l.radius - l.rim - 0.5
                };
                assert!(
                    r <= limit,
                    "{w}x{h} {text}: esquina ({cx},{cy}) r {r} > {limit}"
                );
            }
        }
        let mut fb = frame(w, h);
        draw_oxygen_hud(&mut fb, reading(0.72));
        let top = l.label_origin("O2 72%")[1];
        let lit = (top..top + 7 * l.scale)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.pixels[y * w + x] == HUD_TEXT)
            .count();
        assert!(
            lit > 20 * l.scale * l.scale,
            "{w}x{h}: lectura sin dibujar ({lit})"
        );
    }
}
