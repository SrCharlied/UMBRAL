//! Supervisor-prepared first tracer bullet for UMB-308; no private models or textures.
use std::f64::consts::{PI, TAU};
use umbral::{
    math::{Vec2, Vec3},
    renderer::shader::{FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment},
};

#[test]
fn critical_lucid_surface_is_not_the_lit_planet_placeholder() {
    let material = Material::solid(0.22, 0.23, 0.26);
    let context = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: 0.05,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    };
    let mut distinct = false;
    for longitude in 0..16 {
        for latitude in 1..10 {
            let u = f64::from(longitude) / 16.0;
            let v = f64::from(latitude) / 10.0;
            let angle = (v - 0.5) * PI;
            let normal = Vec3::new(
                angle.cos() * (u * TAU).cos(),
                angle.sin(),
                angle.cos() * (u * TAU).sin(),
            );
            let fragment = FragmentInput {
                screen_position: Vec3::ZERO,
                world_position: normal,
                normal,
                uv: Vec2::new(u, v),
            };
            let lucid = shade_fragment(ShaderKind::LucidSurface, &fragment, &context, &material);
            let lit = shade_fragment(ShaderKind::LitPlanet, &fragment, &context, &material);
            distinct |= (lucid - lit).length() > 1e-9;
        }
    }
    assert!(
        distinct,
        "UMB-308 must provide a real oxygen-driven surface response, not alias LitPlanet"
    );
}

// --- UMB-308, incrementos RED/GREEN y guardas de Bronya; ver logs y salvedades en la doc. ---

use umbral::{
    assets::primitives::uv_sphere,
    math::Transform,
    renderer::{framebuffer::Framebuffer, pipeline::draw_model, stats::RenderStats},
    scene::camera::Camera,
};

const STONE: (f64, f64, f64) = (0.22, 0.23, 0.26);

fn context(oxygen01: f64, elapsed_time: f64) -> ShaderContext {
    ShaderContext {
        elapsed_time,
        oxygen01,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    }
}

/// Fragmento de la esfera unidad en `(u, v)` con la misma convencion que `uv_sphere`.
fn fragment(u: f64, v: f64) -> FragmentInput {
    let (sin_t, cos_t) = (v * PI).sin_cos();
    let (sin_p, cos_p) = (u * TAU).sin_cos();
    let p = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
    FragmentInput {
        screen_position: Vec3::ZERO,
        world_position: p,
        normal: p,
        uv: Vec2::new(u, v),
    }
}

fn shade(kind: ShaderKind, f: &FragmentInput, c: &ShaderContext) -> Vec3 {
    let m = Material::solid(STONE.0, STONE.1, STONE.2);
    shade_fragment(kind, f, c, &m)
}

/// Rejilla latitud-longitud sin tocar los polos exactos.
fn grid() -> impl Iterator<Item = FragmentInput> {
    (0..256)
        .flat_map(|i| (0..128).map(move |j| fragment(i as f64 / 256.0, (j as f64 + 0.5) / 128.0)))
}

fn luminance(c: Vec3) -> f64 {
    0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

#[test]
fn low_oxygen_draws_localized_red_amber_cracks_not_a_global_tint() {
    let c = context(0.05, 1.3);
    let (mut total, mut same, mut lit_up, mut strong) = (0usize, 0usize, 0usize, 0usize);
    for f in grid() {
        let delta = shade(ShaderKind::LucidSurface, &f, &c) - shade(ShaderKind::LitPlanet, &f, &c);
        total += 1;
        if delta.length() < 1e-12 {
            same += 1;
        }
        if luminance(delta) > 1e-6 {
            lit_up += 1;
            assert!(
                delta.x > delta.y && delta.y > delta.z,
                "el acento debe ser rojo-ambar: {delta:?} en uv {:?}",
                f.uv
            );
        }
        if luminance(delta) > 0.08 {
            strong += 1;
        }
    }
    let share = |n: usize| n as f64 / total as f64;
    println!(
        "O2 5 %: intacto {:.3}, con acento {:.3}, acento fuerte {:.3}",
        share(same),
        share(lit_up),
        share(strong)
    );
    assert!(
        share(same) >= 0.40,
        "la piedra entre grietas debe quedar intacta"
    );
    assert!(share(lit_up) <= 0.35, "no debe volver rojo todo el planeta");
    assert!(share(strong) >= 0.02, "las grietas deben verse a O2 5 %");
}

/// Esfera del juego (LOD 0) con `draw_model` real; devuelve pixeles y estadisticas.
fn render(kind: ShaderKind, c: &ShaderContext, yaw: f64) -> (Vec<u32>, RenderStats) {
    let (w, h) = (320, 180);
    let mut fb = Framebuffer::new(w, h, 0x03040a).unwrap();
    let mut camera = Camera::new(60f64.to_radians(), w as f64 / h as f64, 0.1, 100.0).unwrap();
    camera.position = Vec3::new(0.0, 0.0, 2.6);
    camera.target = Vec3::ZERO;
    let transform = Transform {
        rotation: Vec3::new(0.0, yaw, 0.0),
        ..Transform::IDENTITY
    };
    let material = Material {
        cull_backfaces: true,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    let stats = draw_model(
        &mut fb,
        &uv_sphere(32, 64),
        &transform,
        &material,
        kind,
        c,
        &camera,
    );
    (fb.pixels, stats)
}

#[test]
fn oxygen_at_or_above_half_is_exactly_the_lit_planet() {
    for oxygen in [1.0, 0.9, 0.75, 0.6, 0.5] {
        for time in [0.0, 1.3, 7.9, 600.0] {
            let c = context(oxygen, time);
            for f in grid() {
                let lucid = shade(ShaderKind::LucidSurface, &f, &c);
                let lit = shade(ShaderKind::LitPlanet, &f, &c);
                assert_eq!(
                    (lucid.x.to_bits(), lucid.y.to_bits(), lucid.z.to_bits()),
                    (lit.x.to_bits(), lit.y.to_bits(), lit.z.to_bits()),
                    "O2 {oxygen} t {time} uv {:?}",
                    f.uv
                );
            }
        }
        let c = context(oxygen, 1.3);
        let (lucid, lucid_stats) = render(ShaderKind::LucidSurface, &c, 0.4);
        let (lit, lit_stats) = render(ShaderKind::LitPlanet, &c, 0.4);
        assert!(lucid == lit, "render distinto a O2 {oxygen}");
        assert_eq!(lucid_stats, lit_stats);
    }
}

/// Suma del cambio frente a LitPlanet en toda la rejilla (intensidad total del acento).
fn accent(oxygen: f64, time: f64) -> f64 {
    let c = context(oxygen, time);
    grid()
        .map(|f| {
            (shade(ShaderKind::LucidSurface, &f, &c) - shade(ShaderKind::LitPlanet, &f, &c))
                .length()
        })
        .sum()
}

#[test]
fn oxygen_response_is_continuous_monotonic_saturated_and_stateless() {
    // Barrido fino de 60 % a 0 %: ningun salto de color entre pasos de 0.1 %, en especial
    // al cruzar 50 % y 25 %.
    let fragments: Vec<_> = grid().step_by(7).collect();
    let mut worst = (0.0f64, 0.0f64);
    let mut previous: Option<Vec<Vec3>> = None;
    for step in (0..=600).rev() {
        let oxygen = step as f64 / 1000.0;
        let c = context(oxygen, 1.3);
        let now: Vec<_> = fragments
            .iter()
            .map(|f| shade(ShaderKind::LucidSurface, f, &c))
            .collect();
        if let Some(prev) = &previous {
            let jump = prev
                .iter()
                .zip(&now)
                .map(|(a, b)| (*a - *b).length())
                .fold(0.0, f64::max);
            if jump > worst.0 {
                worst = (jump, oxygen);
            }
        }
        previous = Some(now);
    }
    println!(
        "mayor cambio entre pasos de 0.1 % de O2: {:.5} en {:.3}",
        worst.0, worst.1
    );
    assert!(
        worst.0 < 0.02,
        "salto de color {:.4} en O2 {:.3}",
        worst.0,
        worst.1
    );

    // Crece al bajar el oxigeno y se satura en 5 %.
    let levels = [0.5, 0.45, 0.40, 0.25, 0.10, 0.05];
    let values: Vec<_> = levels.iter().map(|&o| accent(o, 1.3)).collect();
    println!("acento por O2 {levels:?}: {values:?}");
    assert_eq!(values[0], 0.0);
    assert!(values.windows(2).all(|w| w[1] > w[0]), "{values:?}");
    assert_eq!(accent(0.0, 1.3).to_bits(), values[5].to_bits());

    // Sin historial: bajar y volver a subir da exactamente lo mismo en cada nivel.
    let f = fragment(0.31, 0.42);
    let down: Vec<_> = levels
        .iter()
        .map(|&o| shade(ShaderKind::LucidSurface, &f, &context(o, 2.0)))
        .collect();
    let up: Vec<_> = levels
        .iter()
        .rev()
        .map(|&o| shade(ShaderKind::LucidSurface, &f, &context(o, 2.0)))
        .collect();
    for (a, b) in down.iter().zip(up.iter().rev()) {
        assert_eq!(
            (a.x.to_bits(), a.y.to_bits(), a.z.to_bits()),
            (b.x.to_bits(), b.y.to_bits(), b.z.to_bits())
        );
    }
}

#[test]
fn cracks_add_no_seam_at_u_wrap_and_no_singularity_at_the_poles() {
    for time in [0.0, 1.3, 4.0] {
        let c = context(0.05, time);
        // Costura: u = 0 y u = 1 son el mismo meridiano; a 1e-9 de cada lado, continuo (un salto
        // real no se achica al acercar los puntos; el borde de una grieta tiene pendiente ~30).
        for j in 0..400 {
            let v = (j as f64 + 0.5) / 400.0;
            let at = |u: f64| shade(ShaderKind::LucidSurface, &fragment(u, v), &c);
            assert!((at(0.0) - at(1.0)).length() < 1e-9, "costura en v {v}");
            assert!(
                (at(1.0 - 1e-9) - at(1e-9)).length() < 1e-4,
                "salto en la costura en v {v}"
            );
        }
        // Polos: alrededor de cada polo la superficie lucida no varia mas que LitPlanet
        // (que ya trae su abanico propio en el polo) salvo una tolerancia pequena.
        for v in [1e-4, 1.0 - 1e-4] {
            let ring: Vec<_> = (0..128).map(|i| fragment(i as f64 / 128.0, v)).collect();
            let spread = |kind| {
                let colors: Vec<_> = ring.iter().map(|f| shade(kind, f, &c)).collect();
                colors
                    .iter()
                    .flat_map(|a| colors.iter().map(move |b| (*a - *b).length()))
                    .fold(0.0, f64::max)
            };
            let (lucid, lit) = (
                spread(ShaderKind::LucidSurface),
                spread(ShaderKind::LitPlanet),
            );
            println!("polo v {v}: dispersion lucida {lucid:.5}, LitPlanet {lit:.5}");
            assert!(
                lucid <= lit + 0.01,
                "singularidad en el polo v {v}: {lucid} frente a {lit}"
            );
        }
    }
}

/// Gira un vector alrededor de +Y.
fn turn(p: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(c * p.x + s * p.z, p.y, -s * p.x + c * p.z)
}

#[test]
fn cracks_are_anchored_to_the_planet_uv_not_to_world_space() {
    // Girar el planeta (posicion y normal) junto con la luz y la camara no cambia el color de
    // un punto de la superficie: el patron viaja con la rotacion.
    for angle in [0.7, 2.1, -1.4] {
        let c = context(0.05, 1.3);
        let turned = ShaderContext {
            light_position: turn(c.light_position, angle),
            camera_position: turn(c.camera_position, angle),
            ..c
        };
        for f in grid().step_by(5) {
            let g = FragmentInput {
                world_position: turn(f.world_position, angle),
                normal: turn(f.normal, angle),
                ..f
            };
            let a = shade(ShaderKind::LucidSurface, &f, &c);
            let b = shade(ShaderKind::LucidSurface, &g, &turned);
            assert!(
                (a - b).length() < 1e-9,
                "el patron no viajo con la rotacion en uv {:?}",
                f.uv
            );
        }
    }
    // Y, en el mismo punto del espacio, otra UV da otra grieta (no depende de world_position).
    let c = context(0.05, 1.3);
    let f = fragment(0.3, 0.4);
    let moved = grid()
        .step_by(97)
        .filter(|g| {
            let h = FragmentInput { uv: g.uv, ..f };
            let delta = |x: &FragmentInput| {
                shade(ShaderKind::LucidSurface, x, &c) - shade(ShaderKind::LitPlanet, x, &c)
            };
            (delta(&h) - delta(&f)).length() > 1e-3
        })
        .count();
    assert!(moved > 0);
}

#[test]
fn time_only_breathes_the_cracks_slowly_without_moving_or_blacking_out() {
    let fragments: Vec<_> = grid().step_by(3).collect();
    let lit: Vec<_> = fragments
        .iter()
        .map(|f| shade(ShaderKind::LitPlanet, f, &context(0.05, 0.0)))
        .collect();
    let delta_at = |time: f64| -> Vec<Vec3> {
        let c = context(0.05, time);
        fragments
            .iter()
            .zip(&lit)
            .map(|(f, l)| shade(ShaderKind::LucidSurface, f, &c) - *l)
            .collect()
    };
    let mask = |d: &[Vec3]| d.iter().map(|v| v.length() > 0.0).collect::<Vec<_>>();
    let start = delta_at(0.0);
    let (dt, frames) = (1.0 / 60.0, 20 * 60);
    let (mut totals, mut worst_frame_step, mut previous) = (Vec::new(), 0.0f64, start.clone());
    for frame in 1..=frames {
        let now = delta_at(frame as f64 * dt);
        // Las grietas no se desplazan: el mismo conjunto de puntos tocados en todo momento.
        assert!(
            mask(&now) == mask(&start),
            "el patron se movio en el cuadro {frame}"
        );
        let step = now
            .iter()
            .zip(&previous)
            .map(|(a, b)| (*a - *b).length())
            .fold(0.0, f64::max);
        worst_frame_step = worst_frame_step.max(step);
        totals.push(now.iter().map(|v| luminance(*v).max(0.0)).sum::<f64>());
        previous = now;
    }
    let (low, high) = totals.iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), &t| {
        (lo.min(t), hi.max(t))
    });
    let peaks = totals
        .windows(3)
        .filter(|w| w[1] > w[0] && w[1] >= w[2])
        .count();
    println!(
        "20 s a 60 fps: brillo total {low:.2}..{high:.2} (min/max {:.3}), picos {peaks}, mayor cambio por cuadro {worst_frame_step:.5}",
        low / high
    );
    assert!(high > low * 1.05, "el tiempo debe animar el acento");
    assert!(low >= 0.65 * high, "sin apagones: {low} frente a {high}");
    assert!(
        (2..=7).contains(&peaks),
        "pulso lento (periodo de 3 a 10 s): {peaks} picos en 20 s"
    );
    assert!(
        worst_frame_step < 0.01,
        "sin estroboscopio: {worst_frame_step} por cuadro"
    );
}

fn bits(c: Vec3) -> (u64, u64, u64) {
    (c.x.to_bits(), c.y.to_bits(), c.z.to_bits())
}

#[test]
fn invalid_oxygen_is_stable_and_invalid_time_is_sanitized_without_new_nans() {
    let times = [
        0.0,
        1.3,
        -2.0,
        1e300,
        -1e300,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let oxygens = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -3.0,
        7.0,
        0.3,
        0.05,
    ];
    for f in grid().step_by(11) {
        for &time in &times {
            let lit = shade(ShaderKind::LitPlanet, &f, &context(1.0, time));
            for oxygen in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 7.0] {
                let lucid = shade(ShaderKind::LucidSurface, &f, &context(oxygen, time));
                assert_eq!(
                    bits(lucid),
                    bits(lit),
                    "O2 {oxygen} debe ser estable (LitPlanet)"
                );
            }
            // Fuera de rango finito: se recorta.
            assert_eq!(
                bits(shade(ShaderKind::LucidSurface, &f, &context(-3.0, time))),
                bits(shade(ShaderKind::LucidSurface, &f, &context(0.0, time)))
            );
            for &oxygen in &oxygens {
                let lucid = shade(ShaderKind::LucidSurface, &f, &context(oxygen, time));
                assert!(
                    lucid.x.is_finite() && lucid.y.is_finite() && lucid.z.is_finite(),
                    "NaN/Inf nuevo con O2 {oxygen}, t {time}: {lucid:?}"
                );
            }
        }
        // Tiempo no finito: como t = 0 (sin alterar a los demas shaders).
        for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                bits(shade(ShaderKind::LucidSurface, &f, &context(0.05, time))),
                bits(shade(ShaderKind::LucidSurface, &f, &context(0.05, 0.0)))
            );
        }
    }
}

/// Esfera del juego a la resolucion real (1280x720, FOV 60) vista desde `distance` radios.
fn render_real(kind: ShaderKind, oxygen: f64, distance: f64) -> (Vec<u32>, RenderStats) {
    let (w, h) = (1280, 720);
    let mut fb = Framebuffer::new(w, h, 0x03040a).unwrap();
    let mut camera = Camera::new(60f64.to_radians(), w as f64 / h as f64, 0.1, 100.0).unwrap();
    camera.position = Vec3::new(0.0, 0.0, distance);
    camera.target = Vec3::ZERO;
    let c = ShaderContext {
        camera_position: camera.position,
        light_position: Vec3::new(5.0, 2.0, 1.0),
        ..context(oxygen, 1.3)
    };
    let material = Material {
        cull_backfaces: true,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    let model = uv_sphere(32, 64);
    let stats = draw_model(
        &mut fb,
        &model,
        &Transform::IDENTITY,
        &material,
        kind,
        &c,
        &camera,
    );
    (fb.pixels, stats)
}

#[test]
fn at_game_distance_the_cracks_are_visible_and_the_silhouette_is_unchanged() {
    for distance in [10.0, 2.6] {
        let (lit, lit_stats) = render_real(ShaderKind::LitPlanet, 0.05, distance);
        let (lucid, lucid_stats) = render_real(ShaderKind::LucidSurface, 0.05, distance);
        assert_eq!(lucid_stats, lit_stats, "misma geometria y profundidad");
        let background = |p: &[u32]| p.iter().map(|&c| c == 0x03040a).collect::<Vec<_>>();
        assert!(background(&lucid) == background(&lit), "misma silueta");
        let channel = |c: u32, shift: u32| ((c >> shift) & 0xff) as i32;
        let disc = background(&lit).iter().filter(|b| !**b).count();
        let (mut changed, mut visible, mut red) = (0, 0, 0);
        for (&a, &b) in lucid.iter().zip(&lit) {
            if b == 0x03040a {
                continue;
            }
            changed += usize::from(a != b);
            visible += usize::from(channel(a, 16) - channel(b, 16) >= 20);
            red += usize::from(channel(a, 16) - channel(a, 8) >= 40);
        }
        let share = |n: usize| n as f64 / disc as f64;
        println!(
            "distancia {distance}: disco {disc} px, cambiados {:.3}, acento visible {:.3}, rojizos {:.3}",
            share(changed),
            share(visible),
            share(red)
        );
        assert!(
            share(visible) >= 0.04,
            "acento demasiado debil a distancia {distance}"
        );
        assert!(
            share(changed) <= 0.45,
            "demasiado planeta cambiado a distancia {distance}"
        );
        assert!(
            share(red) <= 0.25,
            "planeta demasiado rojo a distancia {distance}"
        );
    }
}
