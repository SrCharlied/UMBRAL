//! Supervisor tracer: fill the existing iris band, preserving the black pupil.
use umbral::{
    math::{Vec2, Vec3},
    renderer::shader::{FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment},
};

#[test]
fn iris_is_one_continuous_broad_corona_not_separate_thin_rings() {
    let material = Material::solid(0.22, 0.23, 0.26);
    for time in [0.0, 1.3, 2.8] {
        let context = ShaderContext {
            elapsed_time: time,
            oxygen01: 1.0,
            camera_position: Vec3::new(0.0, 0.0, 5.0),
            ship_position: Vec3::new(0.0, 0.0, 10000.0),
            light_position: Vec3::new(4.0, 5.0, 6.0),
        };
        for i in 0..=160 {
            let angle = 0.42 + f64::from(i) * 0.001;
            let normal = Vec3::new(angle.sin(), 0.0, angle.cos());
            let fragment = FragmentInput {
                screen_position: Vec3::ZERO,
                world_position: normal,
                normal,
                uv: Vec2::new(0.3, 0.4),
            };
            let actual = shade_fragment(ShaderKind::VoidEye, &fragment, &context, &material);
            // At these angles the existing iris envelope is fully on and the pupil is off.
            // Subtract the unchanged shaded stone to measure the emitted corona itself.
            let stone = shade_fragment(ShaderKind::LitPlanet, &fragment, &context, &material)
                * (0.6 * 0.65);
            let emitted_red = actual.x - stone.x;
            assert!(
                emitted_red > 0.015,
                "single broad corona must not contain dark inter-ring gaps: angle={angle}, time={time}, emission={emitted_red}"
            );
        }
    }
}

// --- Bronya: oraculo de la variante 02 contra la referencia 01 verificada (fixture intacto). ---

use std::f64::consts::{PI, TAU};
use umbral::math;

// Referencia historica: variantes y helpers sin usar se conservan byte a byte.
#[allow(dead_code)]
#[path = "fixtures/void_v1_shader.rs"]
mod reference;

/// Constantes de la 01 que la 02 debe conservar (pupila y envolvente del iris).
const PUPIL_EDGE: f64 = 0.3 + 0.04;
const IRIS_OUTER: f64 = 0.75;
/// Esfera de radio 1.5 fuera del origen.
const CENTER: Vec3 = Vec3::new(-2.0, 1.0, 3.0);
const RADIUS: f64 = 1.5;

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn fragment(u: f64, v: f64) -> FragmentInput {
    let (sin_t, cos_t) = (v * PI).sin_cos();
    let (sin_p, cos_p) = (u * TAU).sin_cos();
    let normal = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
    FragmentInput {
        screen_position: Vec3::ZERO,
        world_position: CENTER + normal * RADIUS,
        normal,
        uv: Vec2::new(u, v),
    }
}

fn grid() -> Vec<FragmentInput> {
    (0..96)
        .flat_map(|i| (0..48).map(move |j| fragment(i as f64 / 96.0, (j as f64 + 0.5) / 48.0)))
        .collect()
}

fn context(ship: Vec3, time: f64, camera: Vec3, oxygen01: f64) -> ShaderContext {
    ShaderContext {
        elapsed_time: time,
        oxygen01,
        camera_position: camera,
        ship_position: ship,
        light_position: CENTER + Vec3::new(4.0, 5.0, 6.0),
    }
}

fn candidate(f: &FragmentInput, c: &ShaderContext) -> Vec3 {
    shade_fragment(
        ShaderKind::VoidEye,
        f,
        c,
        &Material::solid(0.22, 0.23, 0.26),
    )
}

fn verified01(f: &FragmentInput, c: &ShaderContext) -> Vec3 {
    let rf = reference::FragmentInput {
        screen_position: f.screen_position,
        world_position: f.world_position,
        normal: f.normal,
        uv: f.uv,
    };
    let rc = reference::ShaderContext {
        elapsed_time: c.elapsed_time,
        oxygen01: c.oxygen01,
        camera_position: c.camera_position,
        ship_position: c.ship_position,
        light_position: c.light_position,
    };
    let rm = reference::Material::solid(0.22, 0.23, 0.26);
    reference::shade_fragment(reference::ShaderKind::VoidEye, &rf, &rc, &rm)
}

/// Angulo del fragmento al eje del ojo, con la misma politica geometrica que el shader; `None`
/// si no hay a donde mirar (ojo cerrado).
fn eye_angle(f: &FragmentInput, c: &ShaderContext) -> Option<f64> {
    let offset = c.ship_position - f.world_position;
    let scale = offset.x.abs().max(offset.y.abs()).max(offset.z.abs());
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let to_ship = (offset * (1.0 / scale)).normalized();
    Some(f.normal.normalized().dot(to_ship).clamp(-1.0, 1.0).acos())
}

/// Envolvente del iris de la 01 (que la variante conserva).
fn iris(angle: f64) -> f64 {
    smoothstep(PUPIL_EDGE, PUPIL_EDGE + 0.06, angle)
        * (1.0 - smoothstep(IRIS_OUTER - 0.12, IRIS_OUTER, angle))
}

/// Emision del ojo: el color menos la piedra oscura con la pupila y el iris de la 01.
fn emission(color: Vec3, f: &FragmentInput, c: &ShaderContext, angle: f64) -> Vec3 {
    let pupil = 1.0 - smoothstep(0.3 - 0.04, PUPIL_EDGE, angle);
    let lit = shade_fragment(
        ShaderKind::LitPlanet,
        f,
        c,
        &Material::solid(0.22, 0.23, 0.26),
    );
    color - lit * (0.6 * (1.0 - 0.96 * pupil - 0.35 * iris(angle)))
}

fn bits(c: Vec3) -> (u64, u64, u64) {
    (c.x.to_bits(), c.y.to_bits(), c.z.to_bits())
}

fn ships() -> [Vec3; 4] {
    [
        CENTER + Vec3::new(0.0, 0.3, 1e4),
        CENTER + Vec3::new(-5.0, 0.4, 2.5),
        CENTER + Vec3::new(1.2, 2.0, 1.4),
        CENTER + Vec3::new(0.2, 9.0, -0.5),
    ]
}

#[test]
fn the_pupil_its_soft_edge_and_the_surface_outside_the_iris_are_exactly_01() {
    let (mut inner, mut outer) = (0, 0);
    for ship in ships() {
        for time in [0.0, 0.7, 1.3, 2.8, f64::NAN, 1e9] {
            for (camera, oxygen) in [(CENTER + Vec3::new(0.0, 0.0, 6.0), 1.0), (Vec3::ZERO, 0.2)] {
                let c = context(ship, time, camera, oxygen);
                for f in grid() {
                    let angle = eye_angle(&f, &c).unwrap();
                    if angle <= PUPIL_EDGE - 1e-9 || angle >= IRIS_OUTER + 1e-9 {
                        assert_eq!(
                            bits(candidate(&f, &c)),
                            bits(verified01(&f, &c)),
                            "fuera de la banda del iris: angulo {angle}, nave {ship:?}, t {time}"
                        );
                        inner += usize::from(angle <= PUPIL_EDGE);
                        outer += usize::from(angle >= IRIS_OUTER);
                    }
                }
            }
        }
    }
    println!("puntos comparados: pupila y borde {inner}, fuera del iris {outer}");
    assert!(inner > 1000 && outer > 1000, "{inner} / {outer}");
    // Ojo cerrado (nave coincidente o no finita): identico a la 01.
    for f in grid().into_iter().step_by(17) {
        for ship in [f.world_position, Vec3::new(f64::NAN, 0.0, 0.0)] {
            let c = context(ship, 1.3, Vec3::ZERO, 1.0);
            assert_eq!(bits(candidate(&f, &c)), bits(verified01(&f, &c)));
        }
    }
}

#[test]
fn the_corona_keeps_the_01_band_envelope_palette_and_glow_ceiling() {
    for ship in ships() {
        for time in [0.0, 0.4, 1.3, 2.8, f64::INFINITY] {
            let c = context(ship, time, Vec3::ZERO, 1.0);
            let mut per_time: Option<Vec3> = None;
            for f in grid() {
                let angle = eye_angle(&f, &c).unwrap();
                if angle <= PUPIL_EDGE + 1e-3 || angle >= IRIS_OUTER - 1e-3 {
                    continue;
                }
                let e = emission(candidate(&f, &c), &f, &c, angle);
                // La forma radial es exactamente la envolvente del iris de la 01: un mismo color
                // por instante, escalado por la envolvente (sin estructura propia en la banda).
                let unit = e * (1.0 / iris(angle));
                let first = *per_time.get_or_insert(unit);
                assert!(
                    (unit - first).length() < 1e-9,
                    "la corona sigue la envolvente: angulo {angle}, {unit:?} vs {first:?}"
                );
                // Paleta de la 01: un punto del segmento rojo -> ambar, escalado como mucho por
                // el brillo maximo de la 01 (0.12).
                let r = unit.y / unit.x;
                let k = (0.35 * r - 0.04) / (0.26 - 0.45 * r);
                let s = unit.x / (0.35 + 0.45 * k);
                assert!(
                    (-1e-9..=1.0 + 1e-9).contains(&k),
                    "fuera de la paleta: k {k}"
                );
                assert!(s > 0.0 && s <= 0.12 + 1e-12, "brillo {s}");
                assert!((unit.z - s * (0.02 + 0.04 * k)).abs() < 1e-12, "{unit:?}");
            }
            assert!(per_time.is_some(), "sin puntos de la banda: nave {ship:?}");
        }
    }
}

/// Emision roja a lo largo de un meridiano que sale del centro de la pupila (nave lejana).
fn meridian(time: f64, shade: fn(&FragmentInput, &ShaderContext) -> Vec3) -> Vec<(f64, f64)> {
    let toward = Vec3::new(0.3, 0.2, 1.0).normalized();
    let side = toward.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
    let c = context(CENTER + toward * 1e4, time, Vec3::ZERO, 1.0);
    (0..=1000)
        .map(|i| {
            let t = 0.30 + i as f64 * 0.0005;
            let normal = (toward * t.cos() + side * t.sin()).normalized();
            let f = FragmentInput {
                screen_position: Vec3::ZERO,
                world_position: CENTER + normal * RADIUS,
                normal,
                uv: Vec2::new(0.3, 0.4),
            };
            let angle = eye_angle(&f, &c).unwrap();
            (angle, emission(shade(&f, &c), &f, &c, angle).x)
        })
        .collect()
}

/// Maximos locales (con tolerancia de redondeo) de un perfil.
fn peaks(profile: &[(f64, f64)]) -> usize {
    let mut count = 0;
    let mut rising = false;
    for w in profile.windows(2) {
        if w[1].1 > w[0].1 + 1e-12 {
            rising = true;
        } else if w[1].1 < w[0].1 - 1e-12 {
            count += usize::from(rising);
            rising = false;
        }
    }
    count
}

#[test]
fn the_iris_band_is_one_connected_corona_without_inner_valleys_at_every_phase() {
    let times = (0..24).map(|k| k as f64 * 0.125).chain([1.3, 2.8]);
    for time in times {
        let profile = meridian(time, candidate);
        // Una sola banda conectada: emision positiva en todo el interior de la banda del iris y
        // nula fuera de ella.
        for &(angle, red) in &profile {
            if angle > PUPIL_EDGE + 1e-6 && angle < IRIS_OUTER - 1e-6 {
                assert!(red > 0.0, "hueco en la corona: angulo {angle}, t {time}");
            } else if !(PUPIL_EDGE - 1e-6..=IRIS_OUTER + 1e-6).contains(&angle) {
                assert!(
                    red.abs() < 1e-12,
                    "emision fuera de la banda: {angle}, t {time}"
                );
            }
            // Llena en la meseta en todas las fases (umbral del supervisor).
            if (0.42..=0.58).contains(&angle) {
                assert!(red > 0.015, "corona tenue: angulo {angle}, t {time}, {red}");
            }
        }
        // Sin valles internos: sube, se mantiene y baja; un unico maximo.
        assert!(peaks(&profile) <= 1, "valles internos a t {time}");
    }
    // El oraculo distingue la 01: sus anillos finos tienen varios maximos en la misma banda.
    let old = peaks(&meridian(1.3, verified01));
    println!("maximos de la 01 en la banda: {old}");
    assert!(
        old >= 3,
        "la referencia 01 deberia tener varios anillos: {old}"
    );
}
