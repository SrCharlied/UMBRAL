//! UMB-308 candidate02: supervisor-prepared reference to the verified01 shader.
use std::f64::consts::{PI, TAU};
use umbral::{
    math,
    math::{Vec2, Vec3},
    renderer::shader::{FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment},
};

// Historical reference only: unused variants/helpers are intentionally retained byte-for-byte.
#[allow(dead_code)]
#[path = "fixtures/lucid_v1_shader.rs"]
mod reference;

#[test]
fn five_percent_variant_changes_the_original_closed_plate_pattern() {
    let material = Material::solid(0.22, 0.23, 0.26);
    let original_material = reference::Material::solid(0.22, 0.23, 0.26);
    let context = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: 0.05,
        camera_position: Vec3::new(0.0, 0.35, 2.6),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(5.0, 2.0, 1.0),
    };
    let original_context = reference::ShaderContext {
        elapsed_time: context.elapsed_time,
        oxygen01: context.oxygen01,
        camera_position: context.camera_position,
        ship_position: context.ship_position,
        light_position: context.light_position,
    };
    let (mut changed, mut total) = (0, 0);
    for longitude in 0..128 {
        for latitude in 1..64 {
            let u = f64::from(longitude) / 128.0;
            let v = f64::from(latitude) / 64.0;
            let (sin_t, cos_t) = (v * PI).sin_cos();
            let (sin_p, cos_p) = (u * TAU).sin_cos();
            let point = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
            let f = FragmentInput {
                screen_position: Vec3::ZERO,
                world_position: point,
                normal: point,
                uv: Vec2::new(u, v),
            };
            let original_f = reference::FragmentInput {
                screen_position: f.screen_position,
                world_position: f.world_position,
                normal: f.normal,
                uv: f.uv,
            };
            let candidate = shade_fragment(ShaderKind::LucidSurface, &f, &context, &material);
            let original = reference::shade_fragment(
                reference::ShaderKind::LucidSurface,
                &original_f,
                &original_context,
                &original_material,
            );
            changed += usize::from((candidate - original).length() > 1e-9);
            total += 1;
        }
    }
    assert!(
        changed as f64 / total as f64 > 0.01,
        "candidate02 must change the critical plate pattern, not simply reship01: {changed}/{total}"
    );
}

// ---------------------------------------------------------------------------------------------
// Variante 02 (trabajador): guardas del mecanismo contra la referencia 01.

use umbral::{
    assets::primitives::uv_sphere,
    math::Transform,
    renderer::{framebuffer::Framebuffer, pipeline::draw_model, stats::RenderStats},
    scene::camera::Camera,
};

const STONE: (f64, f64, f64) = (0.22, 0.23, 0.26);

fn sphere_fragment(u: f64, v: f64) -> FragmentInput {
    let (sin_t, cos_t) = (v * PI).sin_cos();
    let (sin_p, cos_p) = (u * TAU).sin_cos();
    let point = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
    FragmentInput {
        screen_position: Vec3::ZERO,
        world_position: point,
        normal: point,
        uv: Vec2::new(u, v),
    }
}

fn to_reference_fragment(f: &FragmentInput) -> reference::FragmentInput {
    reference::FragmentInput {
        screen_position: f.screen_position,
        world_position: f.world_position,
        normal: f.normal,
        uv: f.uv,
    }
}

fn to_reference_context(c: &ShaderContext) -> reference::ShaderContext {
    reference::ShaderContext {
        elapsed_time: c.elapsed_time,
        oxygen01: c.oxygen01,
        camera_position: c.camera_position,
        ship_position: c.ship_position,
        light_position: c.light_position,
    }
}

fn to_reference_material(m: &Material) -> reference::Material {
    reference::Material {
        base_color: m.base_color,
        use_model_colors: m.use_model_colors,
        ambient: m.ambient,
        cull_backfaces: m.cull_backfaces,
    }
}

/// Lucida y LitPlanet del candidato (`original == false`) o de la referencia 01.
fn lucid_and_lit(
    original: bool,
    f: &FragmentInput,
    c: &ShaderContext,
    m: &Material,
) -> (Vec3, Vec3) {
    if original {
        let (rf, rc, rm) = (
            to_reference_fragment(f),
            to_reference_context(c),
            to_reference_material(m),
        );
        (
            reference::shade_fragment(reference::ShaderKind::LucidSurface, &rf, &rc, &rm),
            reference::shade_fragment(reference::ShaderKind::LitPlanet, &rf, &rc, &rm),
        )
    } else {
        (
            shade_fragment(ShaderKind::LucidSurface, f, c, m),
            shade_fragment(ShaderKind::LitPlanet, f, c, m),
        )
    }
}

const MAP_U: usize = 1024;
const MAP_V: usize = 512;

/// Mapa lat-long de la superficie a O2 dado con luz plana (`ambient` 1: la piedra no depende de
/// la luz), para medir la forma de la red. Marca como grieta cualquier punto cuyo color se aparta
/// de LitPlanet en mas de 0.005: incluye la fisura carbonizada dormida, no solo la brillante.
/// Devuelve (es_grieta, area relativa de cada celda).
fn crack_map(original: bool, oxygen: f64) -> (Vec<bool>, Vec<f64>) {
    let material = Material {
        ambient: 1.0,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    let c = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: oxygen,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    };
    let mut crack = Vec::with_capacity(MAP_U * MAP_V);
    let mut area = Vec::with_capacity(MAP_U * MAP_V);
    for j in 0..MAP_V {
        let v = (j as f64 + 0.5) / MAP_V as f64;
        for i in 0..MAP_U {
            let f = sphere_fragment(i as f64 / MAP_U as f64, v);
            let (lucid, lit) = lucid_and_lit(original, &f, &c, &material);
            crack.push((lucid - lit).length() > 0.005);
            area.push((v * PI).sin());
        }
    }
    let total: f64 = area.iter().sum();
    area.iter_mut().for_each(|a| *a /= total);
    (crack, area)
}

/// Componentes conexas (4-vecinos, periodicas en u, y cada fila polar unida por su polo) de las
/// celdas con `crack == wanted`. Devuelve el area relativa de cada componente.
fn components(crack: &[bool], area: &[f64], wanted: bool) -> Vec<f64> {
    let mut label = vec![usize::MAX; crack.len()];
    let mut sizes = Vec::new();
    for start in 0..crack.len() {
        if crack[start] != wanted || label[start] != usize::MAX {
            continue;
        }
        let id = sizes.len();
        let mut size = 0.0;
        let mut stack = vec![start];
        label[start] = id;
        while let Some(k) = stack.pop() {
            size += area[k];
            let (i, j) = (k % MAP_U, k / MAP_U);
            let mut next = vec![
                j * MAP_U + (i + 1) % MAP_U,
                j * MAP_U + (i + MAP_U - 1) % MAP_U,
            ];
            if j > 0 {
                next.push(k - MAP_U);
            }
            if j + 1 < MAP_V {
                next.push(k + MAP_U);
            }
            // Las celdas de la primera y la ultima fila tocan el mismo polo.
            if j == 0 || j + 1 == MAP_V {
                next.extend((0..MAP_U).step_by(16).map(|ii| j * MAP_U + ii));
            }
            for n in next {
                if crack[n] == wanted && label[n] == usize::MAX {
                    label[n] = id;
                    stack.push(n);
                }
            }
        }
        sizes.push(size);
    }
    sizes
}

/// Placas de piedra que la red deja completamente cerradas (componentes de piedra con al menos
/// 0.5 % de la esfera; una placa media de la 01 ocupa ~2 %).
fn closed_plates(crack: &[bool], area: &[f64]) -> usize {
    components(crack, area, false)
        .iter()
        .filter(|&&s| s >= 0.005)
        .count()
}

#[test]
fn critical_variant_opens_most_closed_plates_including_the_charred_network() {
    let (original, area) = crack_map(true, 0.05);
    let (candidate, _) = crack_map(false, 0.05);
    let (before, after) = (
        closed_plates(&original, &area),
        closed_plates(&candidate, &area),
    );
    println!("placas cerradas a O2 5 %: 01 {before}, candidata {after}");
    assert!(
        before >= 30,
        "la referencia 01 deberia cerrar casi todas sus 48 placas: {before}"
    );
    assert!(
        after * 2 <= before,
        "la red debe dejar abiertas al menos la mitad de las placas: {after} de {before}"
    );
}

// Oraculo independiente de la geometria de la 01 (misma receta que su `crack_sites`): sirve para
// localizar cada union triple de la red y mirar si la variante la conserva cerrada.
fn unit_hash(i: usize, salt: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt;
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn plate_sites() -> Vec<Vec3> {
    let n = 48.0;
    let spacing = (4.0 * PI / n).sqrt();
    let golden = PI * (3.0 - 5f64.sqrt());
    (0..48)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / n;
            let r = (1.0 - y * y).sqrt();
            let (sin_a, cos_a) = (golden * i as f64).sin_cos();
            let jitter =
                Vec3::new(unit_hash(i, 1), unit_hash(i, 2), unit_hash(i, 3)) - Vec3::splat(0.5);
            (Vec3::new(r * cos_a, y, r * sin_a) + jitter * (0.7 * spacing)).normalized()
        })
        .collect()
}

fn warp(d: Vec3) -> Vec3 {
    let w = Vec3::new(
        (d.y * 7.3 + d.z * 3.1).sin(),
        (d.z * 6.7 - d.x * 4.3).sin(),
        (d.x * 7.9 + d.y * 2.9).sin(),
    );
    (d + w * 0.04).normalized()
}

/// Inversa del ondulado de la 01 por punto fijo: la direccion cuya imagen es `p`.
fn unwarp(p: Vec3) -> Vec3 {
    let mut d = p;
    for _ in 0..60 {
        d = (d + p - warp(d)).normalized();
    }
    d
}

fn fragment_at(d: Vec3) -> FragmentInput {
    let v = d.y.clamp(-1.0, 1.0).acos() / PI;
    let u = (d.z.atan2(d.x) / TAU).rem_euclid(1.0);
    sphere_fragment(u, v)
}

/// Uniones triples de la red: (vertice en el dominio ondulado, sus tres centros, longitud del
/// brazo mas corto hasta la union vecina).
fn junctions() -> Vec<(Vec3, [Vec3; 3], f64)> {
    let s = plate_sites();
    let mut found = Vec::new();
    for i in 0..s.len() {
        for j in i + 1..s.len() {
            for k in j + 1..s.len() {
                let mut c = (s[j] - s[i]).cross(s[k] - s[i]).normalized();
                if c.dot(s[i]) < 0.0 {
                    c = c * -1.0;
                }
                let r = c.dot(s[i]);
                if s.iter().all(|q| q.dot(c) <= r + 1e-12) {
                    found.push((c, [i, j, k]));
                }
            }
        }
    }
    // Cada brazo (par de placas) termina en la otra union que comparte ese par.
    let shares = |a: [usize; 3], x: usize, y: usize| a.contains(&x) && a.contains(&y);
    found
        .iter()
        .map(|&(c, ids)| {
            let shortest = [(0, 1), (1, 2), (2, 0)]
                .iter()
                .filter_map(|&(a, b)| {
                    found
                        .iter()
                        .filter(|&&(o, other)| o != c && shares(other, ids[a], ids[b]))
                        .map(|&(o, _)| o.dot(c).clamp(-1.0, 1.0).acos())
                        .reduce(f64::min)
                })
                .fold(f64::INFINITY, f64::min);
            (c, [s[ids[0]], s[ids[1]], s[ids[2]]], shortest)
        })
        .collect()
}

/// Estado de una union: grieta visible (brillante o carbonizada, luz plana) en el vertice y,
/// para cada brazo, distancia desde el vertice hasta donde la grieta vuelve a verse (muestras
/// cada 0.005 rad desde 0.01 hasta 0.08; `None` si no reaparece) y si se ve a 1.5 y 3 anchos
/// de grieta (0.03 y 0.06 rad).
fn junction_state(
    original: bool,
    vertex: Vec3,
    sites: &[Vec3; 3],
    oxygen: f64,
) -> (bool, [Option<f64>; 3], [bool; 3]) {
    let material = Material {
        ambient: 1.0,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    let c = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: oxygen,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    };
    let visible = |p: Vec3| {
        let (lucid, lit) = lucid_and_lit(original, &fragment_at(unwarp(p)), &c, &material);
        (lucid - lit).length() > 0.01
    };
    let (mut reach, mut through) = ([None; 3], [false; 3]);
    for (arm, (a, b, other)) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)].into_iter().enumerate() {
        let mut t = (sites[a] - sites[b]).cross(vertex).normalized();
        if t.dot(sites[other]) > 0.0 {
            t = t * -1.0;
        }
        let at = |r: f64| visible((vertex + t * r).normalized());
        reach[arm] = (2..=16).map(|s| s as f64 * 0.005).find(|&r| at(r));
        through[arm] = at(0.03) && at(0.06);
    }
    (visible(vertex), reach, through)
}

/// Cerrada: grieta visible en el vertice y en sus tres brazos.
fn closed(state: (bool, [Option<f64>; 3], [bool; 3])) -> bool {
    state.0 && state.2.iter().all(|&b| b)
}

#[test]
fn critical_variant_breaks_most_triple_junctions_asymmetrically() {
    let all = junctions();
    assert!(
        (90..=94).contains(&all.len()),
        "48 placas forman ~92 uniones: {}",
        all.len()
    );
    // Solo uniones cuyos brazos miden al menos 0.08 rad: las muestras (hasta 0.06) caen en la
    // propia arista y no en la union vecina.
    let eligible: Vec<_> = all.iter().filter(|j| j.2 >= 0.08).collect();
    let (mut closed_before, mut closed_after, mut broken, mut round_holes) = (0, 0, 0, 0);
    for (vertex, sites, _) in &eligible {
        let before = junction_state(true, *vertex, sites, 0.05);
        if !closed(before) {
            println!("01 no cerrada: {before:?}, vertice {vertex:?}");
            continue;
        }
        closed_before += 1;
        let after = junction_state(false, *vertex, sites, 0.05);
        if closed(after) {
            closed_after += 1;
        } else {
            broken += 1;
            // Agujero redondo: el centro se apaga pero los tres brazos siguen (a 0.03 y 0.06
            // rad). Es el patron repetitivo que se quiere evitar; si falta algun brazo la
            // ruptura es asimetrica. (Cerca del vertice, a 0.01 rad, la muestra de un brazo cae
            // dentro de la banda de los otros dos, por eso no se usa para esto.)
            round_holes += usize::from(!after.0 && after.2.iter().all(|&b| b));
        }
    }
    println!(
        "uniones triples a O2 5 %: {} en total, {} con brazos >= 0.08 rad, cerradas en 01 {closed_before}, cerradas en la candidata {closed_after}, rotas {broken} (agujeros redondos {round_holes})",
        all.len(),
        eligible.len()
    );
    assert!(
        eligible.len() >= 60 && closed_before * 20 >= eligible.len() * 19,
        "el oraculo debe reconocer la red de la 01: {closed_before} de {}",
        eligible.len()
    );
    assert!(
        closed_after * 3 <= closed_before,
        "deben romperse al menos dos tercios de las uniones triples: {closed_after} de {closed_before}"
    );
    assert!(
        round_holes * 3 <= broken,
        "las rupturas no deben ser el mismo agujero redondo en cada Y: {round_holes} de {broken}"
    );
}

fn luminance(c: Vec3) -> f64 {
    0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

/// (acento total = suma de la luminancia anadida, cuota de puntos con acento fuerte > 0.08,
/// cuota de puntos con algun cambio) a O2 dado, con la luz de juego sobre la esfera.
fn accent_stats(original: bool, oxygen: f64, time: f64) -> (f64, f64, f64) {
    let material = Material::solid(STONE.0, STONE.1, STONE.2);
    let c = ShaderContext {
        elapsed_time: time,
        oxygen01: oxygen,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    };
    let (mut total, mut strong, mut changed, mut n) = (0.0, 0usize, 0usize, 0usize);
    for i in 0..256 {
        for j in 0..128 {
            let f = sphere_fragment(i as f64 / 256.0, (j as f64 + 0.5) / 128.0);
            let (lucid, lit) = lucid_and_lit(original, &f, &c, &material);
            let added = luminance(lucid - lit);
            total += added.max(0.0);
            strong += usize::from(added > 0.08);
            changed += usize::from((lucid - lit).length() > 1e-12);
            n += 1;
        }
    }
    (total, strong as f64 / n as f64, changed as f64 / n as f64)
}

#[test]
fn critical_variant_is_not_mere_dimming_and_keeps_legible_accents() {
    for time in [0.0, 1.3, 2.25] {
        let (v1_total, v1_strong, v1_changed) = accent_stats(true, 0.05, time);
        let (total, strong, changed) = accent_stats(false, 0.05, time);
        println!(
            "O2 5 %, t {time}: acento total 01 {v1_total:.2} / candidata {total:.2} ({:.3}), acento fuerte {v1_strong:.4} / {strong:.4}, cambiado {v1_changed:.3} / {changed:.3}",
            total / v1_total
        );
        assert!(
            total >= 0.9 * v1_total,
            "no debe limitarse a apagar: acento {total:.2} frente a {v1_total:.2} en la 01"
        );
        assert!(
            strong >= 0.85 * v1_strong,
            "acentos fuertes legibles: {strong:.4} frente a {v1_strong:.4}"
        );
    }
}

/// Mayor salto de color entre muestras vecinas de un circulo alrededor de `vertex` (en el
/// dominio ondulado) con `steps` muestras, a O2 y tiempo dados.
fn ring_jump(vertex: Vec3, radius: f64, steps: usize, oxygen: f64) -> f64 {
    let material = Material::solid(STONE.0, STONE.1, STONE.2);
    let c = ShaderContext {
        elapsed_time: 1.3,
        oxygen01: oxygen,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: vertex * 5.0,
    };
    let e1 = if vertex.y.abs() < 0.9 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    let e1 = e1.cross(vertex).normalized();
    let e2 = vertex.cross(e1);
    let at = |s: usize| {
        let a = s as f64 / steps as f64 * TAU;
        let p = (vertex + (e1 * a.cos() + e2 * a.sin()) * radius).normalized();
        shade_fragment(
            ShaderKind::LucidSurface,
            &fragment_at(unwarp(p)),
            &c,
            &material,
        )
    };
    let mut previous = at(0);
    let mut worst = 0.0f64;
    for s in 1..=steps {
        let now = at(s % steps);
        worst = worst.max((now - previous).length());
        previous = now;
    }
    worst
}

#[test]
fn breaking_the_network_adds_no_color_jumps_around_the_junctions() {
    // Un salto real no se achica al muestrear mas fino; un borde de grieta continuo, si. El borde
    // mas empinado (`1 - smoothstep` de ancho >= 0.02 por el color mas brillante) cambia menos de
    // ~150 por radian, asi que entre muestras vecinas separadas `step` se admite 250 * step.
    let steps = 4000;
    let mut worst = (0.0f64, 0.0, 0.0, 0.0);
    for (vertex, _, _) in junctions() {
        for oxygen in [0.05, 0.1] {
            for radius in [0.005, 0.02, 0.04, 0.06] {
                let jump = ring_jump(vertex, radius, steps, oxygen);
                let allowed = (250.0 * TAU * radius / steps as f64).max(0.004);
                if jump / allowed > worst.0 {
                    worst = (jump / allowed, jump, radius, oxygen);
                }
            }
        }
    }
    println!(
        "mayor salto alrededor de las uniones: {:.5} = {:.3} del admitido (radio {}, O2 {})",
        worst.1, worst.0, worst.2, worst.3
    );
    assert!(worst.0 < 1.0, "salto de color en una union: {worst:?}");
}

fn bits(c: Vec3) -> (u64, u64, u64) {
    (c.x.to_bits(), c.y.to_bits(), c.z.to_bits())
}

/// Fragmentos de la esfera con normales tal cual y perturbadas (la normal interpolada de una
/// malla no coincide con la posicion), en una rejilla que no toca los polos exactos.
fn contract_fragments() -> Vec<FragmentInput> {
    let mut out = Vec::new();
    for i in 0..96 {
        for j in 0..48 {
            let f = sphere_fragment(i as f64 / 96.0, (j as f64 + 0.5) / 48.0);
            out.push(f);
            out.push(FragmentInput {
                normal: (f.normal + Vec3::new(0.13, -0.07, 0.05)).normalized(),
                screen_position: Vec3::new(i as f64, j as f64, 0.3),
                ..f
            });
        }
    }
    out
}

fn test_context(oxygen: f64, time: f64) -> ShaderContext {
    ShaderContext {
        elapsed_time: time,
        oxygen01: oxygen,
        camera_position: Vec3::new(0.0, 0.0, 5.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    }
}

#[test]
fn from_fifteen_percent_up_the_candidate_is_bit_exact_with_the_reference_01() {
    // El tramo critico empieza en 15 %: desde ahi hacia arriba (y por tanto en todo O2 >= 25 %)
    // la candidata es la 01 bit a bit, con varias luces, tiempos, normales y materiales.
    let lights = [
        Vec3::new(5.0, 2.0, 1.0),
        Vec3::new(4.0, 5.0, 6.0),
        Vec3::new(-3.0, -1.0, 2.0),
    ];
    let times = [0.0, 1.3, 2.25, 4.5, 1e6, f64::NAN];
    let oxygens = [1.0, 0.75, 0.5, 0.4, 0.3, 0.25, 0.2, 0.16, 0.15];
    let materials = [
        Material::solid(STONE.0, STONE.1, STONE.2),
        Material {
            ambient: 0.3,
            cull_backfaces: true,
            ..Material::solid(0.5, 0.4, 0.3)
        },
    ];
    let fragments = contract_fragments();
    let mut compared = 0usize;
    for m in &materials {
        for &light in &lights {
            for &time in &times {
                for &oxygen in &oxygens {
                    let c = ShaderContext {
                        light_position: light,
                        camera_position: Vec3::new(0.0, 0.35, 2.6),
                        ..test_context(oxygen, time)
                    };
                    for f in fragments.iter().step_by(3) {
                        let (candidate, _) = lucid_and_lit(false, f, &c, m);
                        let (original, _) = lucid_and_lit(true, f, &c, m);
                        assert_eq!(
                            bits(candidate),
                            bits(original),
                            "distinto de la 01 con O2 {oxygen}, t {time}, uv {:?}",
                            f.uv
                        );
                        compared += 1;
                    }
                }
            }
        }
    }
    println!("comparaciones bit a bit con la 01 (O2 >= 15 %): {compared}");
}

#[test]
fn the_critical_stretch_is_continuous_saturated_and_differs_from_01_only_below_15_percent() {
    let material = Material::solid(STONE.0, STONE.1, STONE.2);
    let fragments: Vec<_> = contract_fragments().into_iter().step_by(5).collect();
    let shaded = |original: bool, oxygen: f64| -> Vec<Vec3> {
        let c = test_context(oxygen, 1.3);
        fragments
            .iter()
            .map(|f| lucid_and_lit(original, f, &c, &material).0)
            .collect()
    };
    // Barrido de 25 % a 0 % en pasos de 0.05 %: el mayor salto queda bajo 0.01 (la mitad del
    // contrato de la 01 para pasos de 0.1 %), en especial al cruzar 15 % y 5 %.
    let (mut worst, mut previous) = ((0.0f64, 0.0), shaded(false, 0.25));
    for step in (0..500).rev() {
        let oxygen = step as f64 / 2000.0;
        let now = shaded(false, oxygen);
        let jump = now
            .iter()
            .zip(&previous)
            .map(|(a, b)| (*a - *b).length())
            .fold(0.0, f64::max);
        if jump > worst.0 {
            worst = (jump, oxygen);
        }
        previous = now;
    }
    println!(
        "tramo final: mayor salto por paso de 0.05 % de O2 {:.5} en {:.4}",
        worst.0, worst.1
    );
    assert!(worst.0 < 0.01, "salto en el tramo final: {worst:?}");
    // Saturado: a 5 % o menos es exactamente igual.
    let full = shaded(false, 0.05);
    for oxygen in [0.04, 0.01, 0.0, -1.0] {
        assert!(
            shaded(false, oxygen)
                .iter()
                .zip(&full)
                .all(|(a, b)| bits(*a) == bits(*b)),
            "no saturado en O2 {oxygen}"
        );
    }
    // Justo debajo de 15 % ya cambia (poco) y a 10 % cambia mas respecto de la 01.
    let differs = |oxygen: f64| {
        shaded(false, oxygen)
            .iter()
            .zip(shaded(true, oxygen))
            .filter(|(a, b)| (**a - *b).length() > 1e-9)
            .count()
    };
    let (at_149, at_10) = (differs(0.149), differs(0.10));
    println!("puntos distintos de la 01: O2 14.9 % {at_149}, O2 10 % {at_10}");
    assert!(at_149 > 0 && at_10 > at_149);
}

#[test]
fn time_only_breathes_the_variant_and_never_moves_or_reopens_what_it_erased() {
    let material = Material::solid(STONE.0, STONE.1, STONE.2);
    let fragments: Vec<_> = contract_fragments().into_iter().step_by(2).collect();
    for oxygen in [0.12, 0.05] {
        let delta = |time: f64| -> Vec<Vec3> {
            let c = test_context(oxygen, time);
            fragments
                .iter()
                .map(|f| {
                    let (lucid, lit) = lucid_and_lit(false, f, &c, &material);
                    lucid - lit
                })
                .collect()
        };
        let mask = |d: &[Vec3]| d.iter().map(|v| v.length() > 0.0).collect::<Vec<_>>();
        let total = |d: &[Vec3]| d.iter().map(|v| luminance(*v).max(0.0)).sum::<f64>();
        let start = delta(0.0);
        for time in [0.4, 1.3, 2.25, 3.7, 4.5, 1e6, f64::MAX, -f64::MAX] {
            let now = delta(time);
            assert!(
                now.iter()
                    .all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite()),
                "NaN/Inf con t {time}"
            );
            assert!(
                mask(&now) == mask(&start),
                "el tiempo movio o reabrio el patron (O2 {oxygen}, t {time})"
            );
            let ratio = total(&now) / total(&start);
            assert!(
                (0.7..=1.0 + 1e-12).contains(&ratio),
                "solo respira (75..100 %): {ratio} con t {time}"
            );
        }
    }
}

#[test]
fn critical_variant_keeps_seam_poles_anchoring_and_invalid_input_contract() {
    let material = Material::solid(STONE.0, STONE.1, STONE.2);
    let shade = |kind, f: &FragmentInput, c: &ShaderContext| shade_fragment(kind, f, c, &material);
    let turn = |p: Vec3, a: f64| {
        let (s, k) = a.sin_cos();
        Vec3::new(k * p.x + s * p.z, p.y, -s * p.x + k * p.z)
    };
    for oxygen in [0.12, 0.1, 0.07] {
        let c = test_context(oxygen, 1.3);
        // Costura en u = 0/1.
        for j in 0..400 {
            let v = (j as f64 + 0.5) / 400.0;
            let at = |u: f64| shade(ShaderKind::LucidSurface, &sphere_fragment(u, v), &c);
            assert!((at(0.0) - at(1.0)).length() < 1e-9, "costura en v {v}");
            assert!(
                (at(1.0 - 1e-9) - at(1e-9)).length() < 1e-4,
                "salto en la costura en v {v}"
            );
        }
        // Polos: no mas dispersion que LitPlanet (+0.01).
        for v in [1e-4, 1.0 - 1e-4] {
            let ring: Vec<_> = (0..128)
                .map(|i| sphere_fragment(i as f64 / 128.0, v))
                .collect();
            let spread = |kind| {
                let colors: Vec<_> = ring.iter().map(|f| shade(kind, f, &c)).collect();
                colors
                    .iter()
                    .flat_map(|a| colors.iter().map(move |b| (*a - *b).length()))
                    .fold(0.0, f64::max)
            };
            assert!(spread(ShaderKind::LucidSurface) <= spread(ShaderKind::LitPlanet) + 0.01);
        }
        // Anclaje a la UV: girar posicion, normal, luz y camara juntas no cambia el color.
        for angle in [0.7, -1.4] {
            let turned = ShaderContext {
                light_position: turn(c.light_position, angle),
                camera_position: turn(c.camera_position, angle),
                ..c
            };
            for f in contract_fragments().iter().step_by(7) {
                let g = FragmentInput {
                    world_position: turn(f.world_position, angle),
                    normal: turn(f.normal, angle),
                    ..*f
                };
                let a = shade(ShaderKind::LucidSurface, f, &c);
                let b = shade(ShaderKind::LucidSurface, &g, &turned);
                assert!((a - b).length() < 1e-9, "no anclado en uv {:?}", f.uv);
            }
        }
    }
    // Entradas invalidas: O2 no finito = LitPlanet bit a bit; tiempo no finito = t 0.
    for f in contract_fragments().iter().step_by(13) {
        let lit = shade(ShaderKind::LitPlanet, f, &test_context(1.0, 1.3));
        for oxygen in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let lucid = shade(ShaderKind::LucidSurface, f, &test_context(oxygen, 1.3));
            assert_eq!(bits(lucid), bits(lit));
        }
        for oxygen in [0.12, 0.05] {
            let at_zero = shade(ShaderKind::LucidSurface, f, &test_context(oxygen, 0.0));
            for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let lucid = shade(ShaderKind::LucidSurface, f, &test_context(oxygen, time));
                assert_eq!(bits(lucid), bits(at_zero));
            }
        }
    }
}

/// Esfera del juego (LOD 0) con `draw_model` real a 1280x720 desde `distance` radios, girada
/// `yaw`, con la luz de la vista tecnica.
fn render_native(
    kind: ShaderKind,
    oxygen: f64,
    distance: f64,
    yaw: f64,
) -> (Vec<u32>, RenderStats) {
    let (w, h) = (1280, 720);
    let mut fb = Framebuffer::new(w, h, 0x03040a).unwrap();
    let mut camera = Camera::new(60f64.to_radians(), w as f64 / h as f64, 0.1, 100.0).unwrap();
    camera.position = Vec3::new(0.0, 0.0, distance);
    camera.target = Vec3::ZERO;
    let c = ShaderContext {
        camera_position: camera.position,
        light_position: Vec3::new(5.0, 2.0, 1.0),
        ..test_context(oxygen, 1.3)
    };
    let material = Material {
        cull_backfaces: true,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    let transform = Transform {
        rotation: Vec3::new(0.0, yaw, 0.0),
        ..Transform::IDENTITY
    };
    let model = uv_sphere(32, 64);
    let stats = draw_model(&mut fb, &model, &transform, &material, kind, &c, &camera);
    (fb.pixels, stats)
}

#[test]
fn native_renders_keep_geometry_and_the_critical_accent_stays_visible_at_game_distance() {
    for (distance, yaw) in [(10.0, 0.0), (2.6, 0.0), (2.6, 35f64.to_radians())] {
        let (lit, lit_stats) = render_native(ShaderKind::LitPlanet, 0.05, distance, yaw);
        let background = |p: &[u32]| p.iter().map(|&c| c == 0x03040a).collect::<Vec<_>>();
        let disc = background(&lit).iter().filter(|b| !**b).count();
        for oxygen in [0.12, 0.05] {
            let (lucid, stats) = render_native(ShaderKind::LucidSurface, oxygen, distance, yaw);
            assert_eq!(stats, lit_stats, "misma geometria y profundidad");
            assert!(background(&lucid) == background(&lit), "misma silueta");
            let red = |c: u32| ((c >> 16) & 0xff) as i32;
            let visible = lucid
                .iter()
                .zip(&lit)
                .filter(|&(&a, &b)| b != 0x03040a && red(a) - red(b) >= 20)
                .count() as f64
                / disc as f64;
            println!(
                "distancia {distance}, giro {yaw:.2}, O2 {oxygen}: acento visible {visible:.3}"
            );
            if oxygen == 0.05 {
                assert!(visible >= 0.03, "acento demasiado debil: {visible}");
            }
        }
    }
}
