//! Supervisor-prepared first UMB-309 tracer: a distinct shader, not a replacement creature.
use std::f64::consts::{PI, TAU};
use umbral::{
    math::{Vec2, Vec3},
    renderer::shader::{FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment},
};

#[test]
fn void_eye_is_not_the_lit_planet_placeholder() {
    let material = Material::solid(0.22, 0.23, 0.26);
    let context = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: 1.0,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::new(0.0, 0.0, 5.0),
        light_position: Vec3::new(4.0, 5.0, 6.0),
    };
    let mut distinct = false;
    for longitude in 0..16 {
        for latitude in 1..10 {
            let u = f64::from(longitude) / 16.0;
            let v = f64::from(latitude) / 10.0;
            let theta = v * PI;
            let normal = Vec3::new(
                theta.sin() * (u * TAU).cos(),
                theta.cos(),
                theta.sin() * (u * TAU).sin(),
            );
            let fragment = FragmentInput {
                screen_position: Vec3::ZERO,
                world_position: normal,
                normal,
                uv: Vec2::new(u, v),
            };
            let eye = shade_fragment(ShaderKind::VoidEye, &fragment, &context, &material);
            let lit = shade_fragment(ShaderKind::LitPlanet, &fragment, &context, &material);
            distinct |= (eye - lit).length() > 1e-9;
        }
    }
    assert!(
        distinct,
        "UMB-309 must be distinct from LitPlanet; an enum alias does not implement the shader"
    );
}

// --- Bronya: ciclos RED/GREEN y guardas de UMB-309 (ver la evidencia del trabajador). ---

/// Esfera unidad fuera del origen: nada del ojo puede suponer que el cuerpo esta centrado en 0.
const CENTER: Vec3 = Vec3::new(10.0, -3.0, 2.0);

fn sphere(u: f64, v: f64) -> FragmentInput {
    let (sin_t, cos_t) = (v * PI).sin_cos();
    let (sin_p, cos_p) = (u * TAU).sin_cos();
    let normal = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
    FragmentInput {
        screen_position: Vec3::ZERO,
        world_position: CENTER + normal,
        normal,
        uv: Vec2::new(u, v),
    }
}

fn grid() -> Vec<FragmentInput> {
    (0..128)
        .flat_map(|i| (0..64).map(move |j| sphere(i as f64 / 128.0, (j as f64 + 0.5) / 64.0)))
        .collect()
}

fn ctx(ship: Vec3, time: f64) -> ShaderContext {
    ShaderContext {
        elapsed_time: time,
        oxygen01: 1.0,
        camera_position: CENTER + Vec3::new(0.0, 0.0, 6.0),
        ship_position: ship,
        light_position: CENTER + Vec3::new(4.0, 5.0, 6.0),
    }
}

fn shade(kind: ShaderKind, f: &FragmentInput, c: &ShaderContext) -> Vec3 {
    shade_fragment(kind, f, c, &Material::solid(0.22, 0.23, 0.26))
}

fn luminance(c: Vec3) -> f64 {
    0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

/// Pupila: puntos donde el ojo deja menos de un 25 % de la luminancia de `LitPlanet` (una
/// sombra relativa, independiente de la luz). Devuelve (cantidad, direccion media de su normal).
fn pupil(ship: Vec3, time: f64) -> (usize, Vec3) {
    let c = ctx(ship, time);
    let (mut count, mut sum) = (0, Vec3::ZERO);
    for f in grid() {
        let eye = luminance(shade(ShaderKind::VoidEye, &f, &c));
        let lit = luminance(shade(ShaderKind::LitPlanet, &f, &c));
        if eye < 0.25 * lit {
            count += 1;
            sum += f.normal;
        }
    }
    (count, sum.normalized())
}

#[test]
fn the_pupil_faces_the_ship_left_or_right_wherever_the_sphere_is() {
    for (name, offset) in [
        ("izquierda", Vec3::new(-4.0, 0.0, 2.0)),
        ("derecha", Vec3::new(4.0, 0.0, 2.0)),
        ("arriba y lejos", Vec3::new(0.0, 30.0, 10.0)),
    ] {
        let ship = CENTER + offset;
        let (count, axis) = pupil(ship, 1.3);
        let toward = offset.normalized();
        let angle = axis.dot(toward).clamp(-1.0, 1.0).acos().to_degrees();
        println!(
            "nave {name}: pupila {count} puntos, a {angle:.1} grados de la direccion de la nave"
        );
        assert!(count > 0, "nave {name}: debe haber una pupila oscura");
        assert!(
            count < grid().len() / 4,
            "nave {name}: la pupila no es toda la esfera"
        );
        assert!(
            angle < 10.0,
            "nave {name}: la pupila debe mirar a la nave ({angle} grados)"
        );
    }
    let (_, left) = pupil(CENTER + Vec3::new(-4.0, 0.0, 2.0), 1.3);
    let (_, right) = pupil(CENTER + Vec3::new(4.0, 0.0, 2.0), 1.3);
    assert!(left.x < -0.5 && right.x > 0.5, "{left:?} {right:?}");
}

/// Perfil radial: color del ojo a `angle` radianes del centro de la pupila, a lo largo de un
/// meridiano que sale del punto que mira a una nave lejana (direccion casi constante).
fn radial(angle: f64, time: f64) -> Vec3 {
    let toward = Vec3::new(0.3, 0.2, 1.0).normalized();
    let side = toward.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
    let normal = (toward * angle.cos() + side * angle.sin()).normalized();
    let f = FragmentInput {
        screen_position: Vec3::ZERO,
        world_position: CENTER + normal,
        normal,
        uv: Vec2::new(0.3, 0.4),
    };
    shade(ShaderKind::VoidEye, &f, &ctx(CENTER + toward * 1e4, time))
}

fn profile(time: f64) -> Vec<f64> {
    (0..=600)
        .map(|i| luminance(radial(0.35 + i as f64 * 0.001, time)))
        .collect()
}

#[test]
fn the_single_corona_breathes_slowly_in_place_while_the_pupil_stays_on_the_ship() {
    // Indices de `profile`: angulo 0.35 + i * 0.001. Meseta del iris (envolvente = 1):
    // 0.40..0.63 rad; fuera del iris: >= 0.75 rad.
    let (plateau, outside) = (55..=275, 405..=600);
    let (a, b) = (profile(1.3), profile(2.8));
    // Respiracion: medio periodo despues la corona cambia de brillo, toda a la vez y en la
    // misma cantidad (sin onda radial ni anillos que se desplacen).
    let diff: Vec<f64> = plateau.clone().map(|i| b[i] - a[i]).collect();
    let (lo, hi) = diff
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &d| {
            (lo.min(d), hi.max(d))
        });
    println!("cambio de luminancia de la corona en medio periodo: {lo:.6}..{hi:.6}");
    assert!(
        lo.abs() > 1e-3,
        "la corona debe respirar con el tiempo: {lo}"
    );
    assert!(hi - lo < 1e-12, "toda la corona cambia igual: {lo}..{hi}");
    // Fuera del iris nada cambia con el tiempo.
    for i in outside {
        assert_eq!(a[i].to_bits(), b[i].to_bits(), "fuera del iris, indice {i}");
    }
    // Lenta, de un solo ciclo de 3 s: periodica, sin latidos rapidos y con un unico maximo y un
    // unico minimo por periodo.
    let at = |time: f64| luminance(radial(0.5, time));
    let samples: Vec<f64> = (0..60).map(|k| at(k as f64 * 0.05)).collect();
    let (min, max) = samples
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &x| {
            (lo.min(x), hi.max(x))
        });
    for k in 0..60 {
        let (prev, here, next) = (samples[(k + 59) % 60], samples[k], samples[(k + 1) % 60]);
        assert!(
            (next - here).abs() <= 0.1 * (max - min),
            "sin latidos rapidos: t {}",
            k as f64 * 0.05
        );
        let extremum = (here > prev && here >= next) || (here < prev && here <= next);
        assert!(
            !extremum || here == max || here == min,
            "un solo ciclo por periodo: t {}",
            k as f64 * 0.05
        );
        assert!(
            (at(k as f64 * 0.05 + 3.0) - here).abs() < 1e-9,
            "periodo de 3 s: t {}",
            k as f64 * 0.05
        );
    }
    // El acento sigue siendo rojo/ambar en ambas fases.
    for time in [1.3, 2.8] {
        let corona = radial(0.5, time);
        assert!(
            corona.x > corona.y && corona.y > corona.z,
            "acento rojo/ambar: {corona:?}"
        );
    }
    // La pupila y su borde suave no cambian con el tiempo y siguen mirando a la nave.
    for k in 0..=67 {
        let angle = k as f64 * 0.005;
        let still = bits(radial(angle, 0.0));
        for time in [0.7, 1.3, 2.8, 1000.0] {
            assert_eq!(
                bits(radial(angle, time)),
                still,
                "pupila a {angle} rad, t {time}"
            );
        }
    }
    for time in [0.0, 0.7, 2.9, 1000.0] {
        let (count, axis) = pupil(CENTER + Vec3::new(-4.0, 0.0, 2.0), time);
        let toward = Vec3::new(-4.0, 0.0, 2.0).normalized();
        assert!(count > 0 && axis.dot(toward) > 0.98, "t {time}: {axis:?}");
    }
}

fn bits(c: Vec3) -> (u64, u64, u64) {
    (c.x.to_bits(), c.y.to_bits(), c.z.to_bits())
}

fn finite(c: Vec3) -> bool {
    c.x.is_finite() && c.y.is_finite() && c.z.is_finite()
}

#[test]
fn adverse_ship_and_time_fall_back_stably_with_finite_results() {
    for f in grid().into_iter().step_by(37) {
        // Nave coincidente con el fragmento o no finita: el ojo no tiene a donde mirar y queda
        // cerrado, igual en todos esos casos (sin pupila ni iris).
        let closed = shade(ShaderKind::VoidEye, &f, &ctx(f.world_position, 1.3));
        assert!(finite(closed), "nave coincidente: {closed:?}");
        for ship in [
            Vec3::new(f64::NAN, 0.0, 0.0),
            Vec3::new(0.0, f64::INFINITY, 0.0),
            Vec3::new(0.0, 0.0, f64::NEG_INFINITY),
        ] {
            let eye = shade(ShaderKind::VoidEye, &f, &ctx(ship, 1.3));
            assert_eq!(bits(eye), bits(closed), "nave {ship:?}");
        }
        // Tiempo no finito: como t = 0 (solo en este shader). Tiempo enorme: finito.
        let ship = CENTER + Vec3::new(-4.0, 0.0, 2.0);
        let at_zero = shade(ShaderKind::VoidEye, &f, &ctx(ship, 0.0));
        for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let eye = shade(ShaderKind::VoidEye, &f, &ctx(ship, time));
            assert_eq!(bits(eye), bits(at_zero), "t {time}");
        }
        for time in [1e300, f64::MAX, -f64::MAX, -1e9] {
            assert!(
                finite(shade(ShaderKind::VoidEye, &f, &ctx(ship, time))),
                "t {time}"
            );
        }
    }
    // Nave finita pero enorme (sin desbordar al normalizar): la pupila la sigue mirando.
    let far = Vec3::new(-1e300, 0.0, 5e299);
    let (count, axis) = pupil(far, 1.3);
    assert!(
        count > 0 && axis.dot(far.normalized_big()) > 0.98,
        "{axis:?}"
    );
}

trait Big {
    fn normalized_big(self) -> Vec3;
}

impl Big for Vec3 {
    /// Direccion de un vector enorme sin desbordar (escalado antes de normalizar).
    fn normalized_big(self) -> Vec3 {
        let scale = self.x.abs().max(self.y.abs()).max(self.z.abs());
        (self * (1.0 / scale)).normalized()
    }
}

// --- Guardas de preservacion (nacidas en verde; sensibilidad comprobada por mutacion). ---

use umbral::{
    assets::primitives::uv_sphere,
    math::Transform,
    renderer::{framebuffer::Framebuffer, pipeline::draw_model, stats::RenderStats},
    scene::camera::Camera,
};

#[test]
fn void_eye_is_distinct_from_every_other_shader() {
    let c = ShaderContext {
        oxygen01: 0.05,
        ..ctx(CENTER + Vec3::new(-4.0, 0.0, 2.0), 1.3)
    };
    let fragments = grid();
    for other in [
        ShaderKind::Flat,
        ShaderKind::LitPlanet,
        ShaderKind::AnimatedSun,
        ShaderKind::LucidSurface,
    ] {
        let differing = fragments
            .iter()
            .filter(|f| (shade(ShaderKind::VoidEye, f, &c) - shade(other, f, &c)).length() > 1e-3)
            .count();
        println!(
            "{other:?}: {differing} de {} puntos distintos",
            fragments.len()
        );
        assert!(differing * 2 > fragments.len(), "{other:?}: {differing}");
    }
}

#[test]
fn the_eye_ignores_camera_and_oxygen_and_depends_only_on_where_the_ship_is() {
    let ship = CENTER + Vec3::new(-4.0, 0.0, 2.0);
    for f in grid().into_iter().step_by(7) {
        let base = shade(ShaderKind::VoidEye, &f, &ctx(ship, 1.3));
        for camera in [CENTER, Vec3::new(-50.0, 3.0, 9.0), ship] {
            for oxygen in [0.0, 0.05, 0.5, 1.0] {
                let c = ShaderContext {
                    camera_position: camera,
                    oxygen01: oxygen,
                    ..ctx(ship, 1.3)
                };
                assert_eq!(bits(shade(ShaderKind::VoidEye, &f, &c)), bits(base));
            }
        }
    }
}

#[test]
fn moving_the_whole_scene_does_not_move_the_eye_on_the_sphere() {
    // Sin supuesto de origen: trasladar esfera, nave, luz y camara juntas deja el mismo color.
    let ship = CENTER + Vec3::new(3.0, 1.0, 2.0);
    for offset in [Vec3::new(-10.0, 3.0, -2.0), Vec3::new(250.0, -80.0, 400.0)] {
        for f in grid().into_iter().step_by(5) {
            let c = ctx(ship, 2.0);
            let moved_f = FragmentInput {
                world_position: f.world_position + offset,
                ..f
            };
            let moved_c = ShaderContext {
                ship_position: c.ship_position + offset,
                light_position: c.light_position + offset,
                camera_position: c.camera_position + offset,
                ..c
            };
            let a = shade(ShaderKind::VoidEye, &f, &c);
            let b = shade(ShaderKind::VoidEye, &moved_f, &moved_c);
            assert!(
                (a - b).length() < 1e-9,
                "desplazado {offset:?}: {a:?} {b:?}"
            );
        }
    }
}

#[test]
fn the_result_is_pure_deterministic_and_seamless_with_the_pupil_on_the_seam_or_pole() {
    let ship = CENTER + Vec3::new(5.0, 0.4, 0.0); // mira a u = 0 (la costura)
    let c = ctx(ship, 1.3);
    let fragments = grid();
    let forward: Vec<_> = fragments
        .iter()
        .map(|f| bits(shade(ShaderKind::VoidEye, f, &c)))
        .collect();
    let backward: Vec<_> = fragments
        .iter()
        .rev()
        .map(|f| bits(shade(ShaderKind::VoidEye, f, &c)))
        .collect();
    assert!(
        forward.iter().eq(backward.iter().rev()),
        "sin estado entre fragmentos"
    );
    for j in 0..200 {
        let v = (j as f64 + 0.5) / 200.0;
        let at = |u: f64| shade(ShaderKind::VoidEye, &sphere(u, v), &c);
        assert!((at(0.0) - at(1.0)).length() < 1e-9, "costura en v {v}");
    }
    // Pupila sobre el polo: no anade dispersion alrededor del polo sobre la de LitPlanet.
    let over_pole = ctx(CENTER + Vec3::new(0.0, 6.0, 0.0), 1.3);
    for v in [1e-4, 1.0 - 1e-4] {
        let ring: Vec<_> = (0..128).map(|i| sphere(i as f64 / 128.0, v)).collect();
        let spread = |kind| {
            let colors: Vec<_> = ring.iter().map(|f| shade(kind, f, &over_pole)).collect();
            colors
                .iter()
                .flat_map(|a| colors.iter().map(move |b| (*a - *b).length()))
                .fold(0.0, f64::max)
        };
        assert!(spread(ShaderKind::VoidEye) <= spread(ShaderKind::LitPlanet) + 0.01);
    }
}

fn render(kind: ShaderKind, ship: Vec3) -> (Vec<u32>, RenderStats) {
    let (w, h) = (320, 180);
    let mut fb = Framebuffer::new(w, h, 0x03040a).unwrap();
    let mut camera = Camera::new(60f64.to_radians(), w as f64 / h as f64, 0.1, 100.0).unwrap();
    camera.position = Vec3::new(0.0, 0.35, 2.6);
    camera.target = Vec3::ZERO;
    let c = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: 1.0,
        camera_position: camera.position,
        ship_position: ship,
        light_position: Vec3::new(5.0, 2.0, 1.0),
    };
    let material = Material {
        cull_backfaces: true,
        ..Material::solid(0.22, 0.23, 0.26)
    };
    let stats = draw_model(
        &mut fb,
        &uv_sphere(32, 64),
        &Transform::IDENTITY,
        &material,
        kind,
        &c,
        &camera,
    );
    (fb.pixels, stats)
}

#[test]
fn a_real_render_keeps_geometry_and_the_pupil_moves_with_the_ship_on_screen() {
    let (lit, lit_stats) = render(ShaderKind::LitPlanet, Vec3::ZERO);
    let background = |p: &[u32]| p.iter().map(|&c| c == 0x03040a).collect::<Vec<_>>();
    let mut centers = Vec::new();
    for ship in [Vec3::new(-1.6, 0.2, 1.2), Vec3::new(1.6, 0.2, 1.2)] {
        let (eye, stats) = render(ShaderKind::VoidEye, ship);
        assert_eq!(stats, lit_stats, "misma geometria y profundidad");
        assert!(background(&eye) == background(&lit), "misma silueta");
        // Centro en pantalla de los pixeles mucho mas oscuros que LitPlanet (la pupila).
        let lum = |c: u32| ((c >> 16) & 0xff) + ((c >> 8) & 0xff) + (c & 0xff);
        let dark: Vec<usize> = (0..eye.len())
            .filter(|&i| lit[i] != 0x03040a && lum(eye[i]) * 4 < lum(lit[i]))
            .collect();
        assert!(!dark.is_empty());
        centers.push(dark.iter().map(|&i| (i % 320) as f64).sum::<f64>() / dark.len() as f64);
    }
    println!(
        "centro x de la pupila: nave izquierda {:.1}, derecha {:.1}",
        centers[0], centers[1]
    );
    assert!(centers[0] < 150.0 && centers[1] > 170.0, "{centers:?}");
}
