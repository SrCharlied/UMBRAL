use std::f64::consts::FRAC_PI_2;
use umbral::math::{Mat4, Transform, Vec3};
use umbral::scene::solar_system::SolarSystem;
use umbral::visual::procedural_surface::{DistributionSettings, distribute};
use umbral::visual::surface_placement::{
    MAX_ELEVATION, MAX_PROP_SCALE, MAX_WORLD_SCALE, MIN_ELEVATION, MIN_WORLD_SCALE, PropPlacement,
    SurfaceAnchor,
};

fn placement(twist: f64, elevation: f64, scale: f64) -> PropPlacement {
    PropPlacement {
        twist,
        elevation,
        scale,
    }
}

fn assert_close(a: Vec3, b: Vec3, tol: f64, what: &str) {
    assert!((a - b).length() <= tol, "{what}: {a:?} != {b:?}");
}

/// Columnas X, Y, Z de la parte lineal.
fn axes(m: &Mat4) -> [Vec3; 3] {
    [m.column(0), m.column(1), m.column(2)]
}

fn normals() -> Vec<Vec3> {
    vec![
        Vec3::X,
        -Vec3::X,
        Vec3::Y,
        -Vec3::Y,
        Vec3::Z,
        -Vec3::Z,
        // A ambos lados del umbral de respaldo polar.
        Vec3::new(1e-9, 1.0, 0.0).normalized(),
        Vec3::new(-1e-9, -1.0, 1e-9).normalized(),
        Vec3::new(1e-5, 1.0, 0.0).normalized(),
        Vec3::new(1.0, 2.0, 3.0).normalized(),
        Vec3::new(-0.3, 0.2, -0.9).normalized(),
    ]
}

#[test]
fn alinea_y_local_con_la_normal_con_base_ortonormal_derecha_y_pie_en_la_superficie() {
    for n in normals() {
        let anchor = SurfaceAnchor::new(n, placement(0.0, 0.0, 1.0)).unwrap();
        let m = anchor.local_matrix();
        let [x, y, z] = axes(&m);
        assert_close(y, n, 1e-12, "Y local = normal");
        for (axis, name) in [(x, "x"), (z, "z")] {
            assert!(
                (axis.length() - 1.0).abs() < 1e-12,
                "{name} unitario en {n:?}"
            );
        }
        assert!(x.dot(y).abs() < 1e-12 && y.dot(z).abs() < 1e-12 && z.dot(x).abs() < 1e-12);
        assert_close(x.cross(y), z, 1e-12, "mano derecha");
        assert_close(
            m.transform_point(Vec3::ZERO),
            n,
            1e-12,
            "pie sobre la esfera",
        );
        let again = SurfaceAnchor::new(n, placement(0.0, 0.0, 1.0)).unwrap();
        assert_eq!(again.local_matrix(), m, "misma entrada, misma pose");
    }
}

#[test]
fn con_giro_cero_z_local_es_horizontal_y_x_local_apunta_al_norte() {
    for n in normals().into_iter().filter(|n| n.y.abs() < 0.999) {
        let [x, _, z] = axes(
            &SurfaceAnchor::new(n, placement(0.0, 0.0, 1.0))
                .unwrap()
                .local_matrix(),
        );
        assert!(z.y.abs() < 1e-12, "Z horizontal en {n:?}");
        assert!(x.y > 0.0, "X hacia el polo norte en {n:?}");
    }
}

#[test]
fn el_giro_conserva_normal_y_pie_y_rota_la_tangente_alrededor_de_la_normal() {
    let n = Vec3::new(0.4, 0.5, -0.7).normalized();
    let base = SurfaceAnchor::new(n, placement(0.0, 0.0, 1.0))
        .unwrap()
        .local_matrix();
    let [x0, y0, z0] = axes(&base);
    for twist in [0.3, -1.2, std::f64::consts::PI] {
        let m = SurfaceAnchor::new(n, placement(twist, 0.0, 1.0))
            .unwrap()
            .local_matrix();
        let [x, y, z] = axes(&m);
        let (s, c) = twist.sin_cos();
        assert_close(y, y0, 1e-12, "la normal no cambia");
        assert_close(x, x0 * c - z0 * s, 1e-12, "X girada");
        assert_close(z, x0 * s + z0 * c, 1e-12, "Z girada");
        assert!((x - x0).length() > 0.25, "la tangente cambia con {twist}");
        assert_close(m.transform_point(Vec3::ZERO), n, 1e-12, "pie fijo");
    }
    let full = SurfaceAnchor::new(n, placement(std::f64::consts::TAU, 0.0, 1.0)).unwrap();
    for (a, b) in axes(&full.local_matrix()).into_iter().zip(axes(&base)) {
        assert_close(a, b, 1e-12, "giro de 2 pi");
    }
}

#[test]
fn elevacion_y_escala_se_miden_en_radios_del_planeta() {
    let n = Vec3::new(-0.2, -0.6, 0.3).normalized();
    for (elevation, scale) in [(0.1, 0.2), (-0.05, 0.5), (0.0, 0.01)] {
        let m = SurfaceAnchor::new(n, placement(0.7, elevation, scale))
            .unwrap()
            .local_matrix();
        let pivot = n * (1.0 + elevation);
        assert_close(m.transform_point(Vec3::ZERO), pivot, 1e-12, "pivote");
        // Un prop de altura 1 en su modelo mide `scale` radios a lo largo de la normal.
        assert_close(
            m.transform_point(Vec3::Y),
            n * (1.0 + elevation + scale),
            1e-12,
            "punta",
        );
        // La base del modelo (y = 0) queda en el plano tangente del pivote.
        for p in [Vec3::new(0.5, 0.0, 0.0), Vec3::new(-0.3, 0.0, 0.8)] {
            assert!(
                (m.transform_point(p) - pivot).dot(n).abs() < 1e-12,
                "base tangente"
            );
        }
        for axis in axes(&m) {
            assert!((axis.length() - scale).abs() < 1e-12, "escala uniforme");
        }
    }
}

/// Mayor diferencia por elemento de las 3 primeras filas, relativa a la magnitud.
fn matrix_error(a: &Mat4, b: &Mat4) -> f64 {
    (0..3)
        .flat_map(|i| (0..4).map(move |j| (i, j)))
        .map(|(i, j)| (a.m[i][j] - b.m[i][j]).abs() / (1.0 + a.m[i][j].abs()))
        .fold(0.0, f64::max)
}

#[test]
fn sigue_la_orbita_y_rotacion_de_un_cuerpo_celeste_sin_regenerar_anclajes() {
    let system = SolarSystem::demo();
    let body = &system.bodies[1];
    let samples = distribute(&DistributionSettings {
        seed: 305,
        attempts: 32,
        noise_scale: 2.0,
        threshold: 0.0,
        min_spacing: 0.0,
    })
    .unwrap();
    let (elevation, scale) = (0.02, 0.1);
    // Los anclajes se crean una vez; despues solo cambia el tiempo.
    let anchors: Vec<_> = samples
        .iter()
        .map(|s| {
            SurfaceAnchor::new(
                s.normal,
                placement(f64::from(s.attempt) * 2.4, elevation, scale),
            )
            .unwrap()
        })
        .collect();
    let tip_in_model = Vec3::new(0.3, 1.0, -0.2);
    for t in [0.0, 7.5, 31.0] {
        let planet = body.transform(t);
        let center = body.position(t);
        let spin = Mat4::rotation_y(body.rotation_angle(t));
        let unspin = Mat4::rotation_y(-body.rotation_angle(t));
        for (anchor, sample) in anchors.iter().zip(&samples) {
            let m = anchor.world_matrix(&planet).unwrap();
            let contact = m.transform_point(Vec3::ZERO);
            let expected =
                center + spin.transform_dir(sample.normal) * (body.body_radius * (1.0 + elevation));
            assert_close(contact, expected, 1e-9, "contacto en el mundo");
            assert!(
                ((contact - center).length() - body.body_radius * (1.0 + elevation)).abs() < 1e-9
            );
            assert_close(
                m.column(1).normalized(),
                (contact - center).normalized(),
                1e-12,
                "Y = radial",
            );
            // En el marco del planeta, cualquier punto del prop queda fijo en el tiempo.
            let tip =
                unspin.transform_dir(m.transform_point(tip_in_model) - center) / body.body_radius;
            assert_close(
                tip,
                anchor.local_matrix().transform_point(tip_in_model),
                1e-12,
                "fijo al planeta",
            );
        }
    }
}

/// Mayor norma de la diferencia entre columnas 0..2 (ejes escalados) de dos matrices.
fn column_error(a: &Mat4, b: &Mat4) -> f64 {
    (0..3)
        .map(|c| (a.column(c) - b.column(c)).length())
        .fold(0.0, f64::max)
}

#[test]
fn world_transform_reproduce_la_matriz_exacta_en_toda_la_esfera() {
    let planets = [
        Transform::IDENTITY,
        SolarSystem::demo().bodies[2].transform(0.0),
        SolarSystem::demo().bodies[2].transform(12.0),
        Transform {
            position: Vec3::new(5.0, -3.0, 2.0),
            rotation: Vec3::new(0.4, -1.1, 2.3),
            scale: Vec3::splat(2.5),
        },
    ];
    let normals = [
        Vec3::Y,
        -Vec3::Y,
        Vec3::X,
        -Vec3::Z,
        Vec3::new(1.0, 2.0, 3.0).normalized(),
    ];
    let mut cases = 0;
    for planet in &planets {
        for n in normals {
            for twist in [0.0, 0.8, -2.5, FRAC_PI_2, -FRAC_PI_2] {
                let anchor = SurfaceAnchor::new(n, placement(twist, 0.05, 0.2)).unwrap();
                let exact = anchor.world_matrix(planet).unwrap();
                let drawn = anchor.world_transform(planet).unwrap().matrix();
                let error = matrix_error(&exact, &drawn);
                assert!(error < 1e-9, "{planet:?} {n:?} {twist}: {error}");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 100);
}

#[test]
fn world_transform_es_exacto_alrededor_del_bloqueo_de_cardan() {
    // Ecuador: |m12| = |sin(twist)|. Repro de revision: twist = pi/2 - 0.001 daba ~1e-3.
    let tilted = Transform {
        position: Vec3::new(-4.0, 1.0, 7.0),
        rotation: Vec3::new(0.3, 1.2, -0.2),
        scale: Vec3::splat(3.0),
    };
    let mut worst = 0.0_f64;
    for planet in [Transform::IDENTITY, tilted] {
        for sign in [1.0, -1.0] {
            for delta in [
                0.0, 1e-12, -1e-12, 1e-9, -1e-9, 1e-6, -1e-6, 1e-4, -1e-4, 1e-3, -1e-3, 1.4e-3,
                -1.4e-3, 2e-3, -2e-3,
            ] {
                let twist = sign * FRAC_PI_2 + delta;
                let anchor = SurfaceAnchor::new(Vec3::X, placement(twist, 0.0, 1.0)).unwrap();
                let exact = anchor.world_matrix(&planet).unwrap();
                let drawn = anchor.world_transform(&planet).unwrap().matrix();
                let (columns, entries) =
                    (column_error(&exact, &drawn), matrix_error(&exact, &drawn));
                assert!(
                    columns < 1e-9 && entries < 1e-9,
                    "planeta {:?} twist {twist}: columnas {columns:e}, elementos {entries:e}",
                    planet.rotation
                );
                worst = worst.max(columns).max(entries);
            }
        }
    }
    println!("world_transform cerca del gimbal: error maximo {worst:e}");
}

#[test]
fn diagnostico_mat4_to_euler_existente_conserva_su_banda_de_cardan() {
    // Caracteriza la funcion de Persona 1, no `world_transform`: con |m12| >= 0.999999 fija
    // el cabeceo en +-90 grados y el error de reconstruccion sigue a la distancia al gimbal.
    // Si `to_euler` se corrige, este diagnostico fallara: actualizarlo junto con la doc.
    let bound = 0.999_999_f64.acos();
    for (delta, inside) in [(1e-4, true), (1e-3, true), (1.4e-3, true), (2e-3, false)] {
        for sign in [1.0, -1.0] {
            let r = Mat4::from_euler(Vec3::new(sign * (FRAC_PI_2 - delta), 0.7, 0.4));
            assert_eq!(r.m[1][2].abs() >= 0.999_999, inside, "delta {delta}");
            let error = matrix_error(&r, &Mat4::from_euler(r.to_euler()));
            println!("to_euler, signo {sign}, delta {delta:e}: error {error:e}");
            if inside {
                assert!(
                    error >= 0.5 * delta && error <= bound,
                    "delta {delta}: {error}"
                );
            } else {
                assert!(error < 1e-9, "delta {delta}: {error}");
            }
        }
    }
}

fn expect_err<T: std::fmt::Debug>(result: Result<T, String>, field: &str, case: &str) {
    match result {
        Err(error) => assert!(error.contains(field), "{case}: {error}"),
        Ok(value) => panic!("{case}: se acepto {value:?}"),
    }
}

#[test]
fn rechaza_normales_y_colocaciones_invalidas() {
    let ok = placement(0.0, 0.0, 0.1);
    for n in [
        Vec3::ZERO,
        Vec3::new(f64::NAN, 1.0, 0.0),
        Vec3::new(0.0, f64::INFINITY, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(1e300, 0.0, 0.0),
        Vec3::new(1.0, 1e-4, 0.0),
    ] {
        expect_err(SurfaceAnchor::new(n, ok), "normal", &format!("{n:?}"));
    }
    let cases: [(PropPlacement, &str); 12] = [
        (placement(f64::NAN, 0.0, 0.1), "twist"),
        (placement(f64::INFINITY, 0.0, 0.1), "twist"),
        (placement(0.0, f64::NAN, 0.1), "elevation"),
        (placement(0.0, f64::NEG_INFINITY, 0.1), "elevation"),
        (placement(0.0, MIN_ELEVATION - 0.01, 0.1), "elevation"),
        (placement(0.0, MAX_ELEVATION + 0.01, 0.1), "elevation"),
        (placement(0.0, 0.0, 0.0), "scale"),
        (placement(0.0, 0.0, -0.1), "scale"),
        (placement(0.0, 0.0, f64::NAN), "scale"),
        (placement(0.0, 0.0, f64::INFINITY), "scale"),
        (placement(0.0, 0.0, MAX_PROP_SCALE * 1.01), "scale"),
        (placement(0.0, 0.0, -0.0), "scale"),
    ];
    for (p, field) in cases {
        expect_err(SurfaceAnchor::new(Vec3::X, p), field, &format!("{p:?}"));
    }
}

#[test]
fn rechaza_planetas_no_uniformes_reflejados_o_no_finitos() {
    let anchor = SurfaceAnchor::new(Vec3::Y, placement(0.0, 0.0, 0.1)).unwrap();
    let with = |position: Vec3, rotation: Vec3, scale: Vec3| Transform {
        position,
        rotation,
        scale,
    };
    let (p, r) = (Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.1, 0.2, 0.3));
    for (planet, field) in [
        (with(p, r, Vec3::new(1.0, 2.0, 1.0)), "uniforme"),
        (with(p, r, Vec3::splat(0.0)), "finita y positiva"),
        (with(p, r, Vec3::splat(-1.0)), "finita y positiva"),
        (with(p, r, Vec3::splat(f64::NAN)), "finita y positiva"),
        (with(p, r, Vec3::splat(f64::INFINITY)), "finita y positiva"),
        (
            with(Vec3::new(f64::NAN, 0.0, 0.0), r, Vec3::ONE),
            "position",
        ),
        (
            with(Vec3::new(0.0, 0.0, f64::INFINITY), r, Vec3::ONE),
            "position",
        ),
        (
            with(p, Vec3::new(0.0, f64::NAN, 0.0), Vec3::ONE),
            "rotation",
        ),
        (
            with(p, Vec3::new(f64::INFINITY, 0.0, 0.0), Vec3::ONE),
            "rotation",
        ),
    ] {
        expect_err(
            anchor.world_matrix(&planet),
            field,
            &format!("matriz {planet:?}"),
        );
        expect_err(
            anchor.world_transform(&planet),
            field,
            &format!("transform {planet:?}"),
        );
    }
}

#[test]
fn rechaza_contacto_desbordado_aunque_la_escala_mundial_sea_valida() {
    let prop_scale = 1e-200;
    let radius = f64::MAX;
    let world_scale = radius * prop_scale;
    assert!((MIN_WORLD_SCALE..=MAX_WORLD_SCALE).contains(&world_scale));
    let anchor = SurfaceAnchor::new(Vec3::X, placement(0.0, MAX_ELEVATION, prop_scale)).unwrap();
    let planet = Transform {
        scale: Vec3::splat(radius),
        ..Transform::IDENTITY
    };
    expect_err(anchor.world_matrix(&planet), "finita", "contacto matriz");
    expect_err(
        anchor.world_transform(&planet),
        "finita",
        "contacto transform",
    );
}

#[test]
fn rechaza_poses_que_desbordan_y_acepta_extremos_finitos() {
    let high = SurfaceAnchor::new(Vec3::X, placement(0.0, MAX_ELEVATION, MAX_PROP_SCALE)).unwrap();
    // Radios que desbordarian la pose: se rechazan antes, por el dominio de escala en el
    // mundo (MAX_WORLD_SCALE), asi que el error nombra la escala.
    for planet in [
        Transform {
            scale: Vec3::splat(f64::MAX),
            ..Transform::IDENTITY
        },
        Transform {
            position: Vec3::new(f64::MAX, 0.0, 0.0),
            scale: Vec3::splat(1e308),
            ..Transform::IDENTITY
        },
    ] {
        expect_err(
            high.world_matrix(&planet),
            "escala",
            &format!("matriz {planet:?}"),
        );
        expect_err(
            high.world_transform(&planet),
            "escala",
            &format!("transform {planet:?}"),
        );
    }
    // Caso finito con radio acotado: este contacto no alcanza el ulp de f64::MAX.
    let extreme = Transform {
        position: Vec3::new(f64::MAX, -f64::MAX, f64::MAX),
        rotation: Vec3::new(1.0, -2.0, 3.0),
        scale: Vec3::splat(MAX_WORLD_SCALE),
    };
    let m = high.world_matrix(&extreme).unwrap();
    assert!((0..3).all(|i| m.m[i].iter().all(|v| v.is_finite())));
    let t = high.world_transform(&extreme).unwrap();
    assert!(t.position.is_finite() && t.rotation.is_finite() && t.scale.is_finite());
    let planet = Transform {
        position: Vec3::new(1e9, -1e9, 3e8),
        rotation: Vec3::new(0.3, 2.0, -0.4),
        scale: Vec3::splat(1e6),
    };
    for p in [
        placement(1e6, MIN_ELEVATION, MAX_PROP_SCALE),
        placement(-3.0, MAX_ELEVATION, 1e-6),
    ] {
        let anchor = SurfaceAnchor::new(Vec3::new(0.0, -0.6, 0.8), p).unwrap();
        let m = anchor.world_matrix(&planet).unwrap();
        assert!((0..3).all(|i| m.m[i].iter().all(|v| v.is_finite())));
        let t = anchor.world_transform(&planet).unwrap();
        assert!(t.position.is_finite() && t.rotation.is_finite() && t.scale.is_finite());
        let contact = m.transform_point(Vec3::ZERO);
        assert!(((contact - planet.position).length() - 1e6 * (1.0 + p.elevation)).abs() < 1e-3);
    }
}

#[test]
fn la_escala_en_el_mundo_debe_poder_invertirla_el_pipeline() {
    let planet = |r: f64| Transform {
        scale: Vec3::splat(r),
        ..Transform::IDENTITY
    };
    // Hallazgo de revision: 1e-200 * 1e-200 = 0 daba un Transform "finito" pero degenerado.
    // 1e-200 y 1e-300 son f64 normales, pero `transform_normal` eleva 1/s al cuadrado y
    // desborda: el dominio es [MIN_WORLD_SCALE, MAX_WORLD_SCALE].
    for (prop, radius) in [
        (1e-200, 1e-200),
        (1e-10, 1e-300),
        (1e-100, 1e-100),
        (1.0, MIN_WORLD_SCALE / 10.0),
        (1.0, MAX_WORLD_SCALE * 10.0),
    ] {
        let anchor = SurfaceAnchor::new(Vec3::Y, placement(0.0, 0.0, prop)).unwrap();
        let case = format!("prop {prop:e}, radio {radius:e}");
        expect_err(anchor.world_matrix(&planet(radius)), "escala", &case);
        expect_err(anchor.world_transform(&planet(radius)), "escala", &case);
    }
    expect_err(
        SurfaceAnchor::new(Vec3::Y, placement(0.0, 0.0, 1e-310)),
        "scale",
        "prop subnormal",
    );
    // Extremos aceptados: matriz no degenerada y normales que el pipeline puede transformar.
    for radius in [MIN_WORLD_SCALE, MAX_WORLD_SCALE] {
        let anchor = SurfaceAnchor::new(Vec3::Y, placement(0.7, 0.0, 1.0)).unwrap();
        let m = anchor.world_matrix(&planet(radius)).unwrap();
        for a in axes(&m) {
            // Reescalado: `length()` de ejes de 1e-150 subdesborda al elevar al cuadrado.
            assert!(((a / radius).length() - 1.0).abs() < 1e-12, "eje {a:?}");
        }
        let t = anchor.world_transform(&planet(radius)).unwrap();
        for n in [Vec3::X, Vec3::Y, Vec3::Z] {
            let w = t.transform_normal(n);
            assert!(
                (w.length() - 1.0).abs() < 1e-12,
                "radio {radius:e}: normal {w:?}"
            );
        }
    }
}
