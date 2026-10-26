use umbral::math::Vec3;
use umbral::visual::procedural_surface::{
    DistributionSettings, MAX_ATTEMPTS, MAX_NOISE_COORD, MAX_NOISE_SCALE, SurfaceSample, density,
    distribute,
};

fn settings(seed: u64) -> DistributionSettings {
    DistributionSettings {
        seed,
        attempts: MAX_ATTEMPTS,
        noise_scale: 3.0,
        threshold: 0.0,
        min_spacing: 0.0,
    }
}

#[test]
fn misma_semilla_misma_salida_y_otra_semilla_distinta() {
    let a = distribute(&settings(24531)).unwrap();
    let b = distribute(&settings(24531)).unwrap();
    let c = distribute(&settings(24532)).unwrap();
    assert!(!a.is_empty());
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn muestras_unitarias_con_normal_radial_y_sin_sesgo_polar() {
    let samples = distribute(&settings(7)).unwrap();
    assert_eq!(samples.len(), MAX_ATTEMPTS as usize, "umbral 0 acepta todo");
    for (i, s) in samples.iter().enumerate() {
        assert_eq!(s.attempt as usize, i, "orden determinista por intento");
        assert!(s.position.is_finite() && s.normal.is_finite());
        assert!((s.position.length() - 1.0).abs() < 1e-12);
        assert!((s.normal.length() - 1.0).abs() < 1e-12);
        assert!((s.normal - s.position).length() < 1e-12, "normal radial");
    }
    // Uniforme en area: |y| > 0.5 cubre la mitad de la esfera (un muestreo uniforme en
    // latitud daria dos tercios: |latitud| > 30 grados), y cada hemisferio recibe la mitad.
    let n = samples.len() as f64;
    let caps = samples.iter().filter(|s| s.position.y.abs() > 0.5).count() as f64 / n;
    let north = samples.iter().filter(|s| s.position.y > 0.0).count() as f64 / n;
    let east = samples.iter().filter(|s| s.position.x > 0.0).count() as f64 / n;
    assert!((caps - 0.5).abs() < 0.04, "fraccion en casquetes: {caps}");
    assert!((north - 0.5).abs() < 0.04, "hemisferio norte: {north}");
    assert!((east - 0.5).abs() < 0.04, "hemisferio este: {east}");
}

#[test]
fn densidad_acotada_continua_y_dependiente_de_semilla_y_escala() {
    let points: Vec<Vec3> = distribute(&settings(11))
        .unwrap()
        .iter()
        .map(|s| s.position)
        .collect();
    let field = |seed, scale| -> Vec<f64> {
        points
            .iter()
            .map(|&p| density(seed, scale, p).unwrap())
            .collect()
    };
    let base = field(11, 3.0);
    assert!(base.iter().all(|d| (0.0..=1.0).contains(d)));
    assert_eq!(base, field(11, 3.0), "reproducible");
    let (min, max) = base
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &d| {
            (a.min(d), b.max(d))
        });
    assert!(max - min > 0.4, "campo casi constante: {min}..{max}");
    let differing = |other: Vec<f64>| base.iter().zip(&other).filter(|(a, b)| a != b).count();
    assert!(
        differing(field(12, 3.0)) > points.len() * 9 / 10,
        "otra semilla, otro campo"
    );
    assert!(
        differing(field(11, 6.0)) > points.len() * 9 / 10,
        "otra escala, otro campo"
    );
    // Coherencia espacial: un desplazamiento minusculo apenas cambia la densidad.
    for &p in points.iter().take(256) {
        let q = p + Vec3::new(1e-6, -1e-6, 1e-6);
        assert!((density(11, 3.0, p).unwrap() - density(11, 3.0, q).unwrap()).abs() < 1e-3);
    }
}

#[test]
fn el_umbral_filtra_por_densidad_y_conserva_subconjuntos() {
    let all = distribute(&settings(24531)).unwrap();
    let mut previous: Option<Vec<SurfaceSample>> = None;
    for threshold in [0.25, 0.5, 0.75] {
        let kept = distribute(&DistributionSettings {
            threshold,
            ..settings(24531)
        })
        .unwrap();
        let expected: Vec<_> = all
            .iter()
            .copied()
            .filter(|s| s.density >= threshold)
            .collect();
        assert_eq!(kept, expected, "umbral {threshold}");
        if let Some(previous) = previous {
            assert!(
                kept.iter().all(|s| previous.contains(s)),
                "subconjunto en {threshold}"
            );
            assert!(kept.len() < previous.len());
        }
        previous = Some(kept);
    }
    let middle = all.iter().filter(|s| s.density >= 0.5).count();
    assert!(middle > 0 && middle < all.len());
}

#[test]
fn el_espaciado_minimo_cordal_se_respeta_de_forma_voraz_y_ordenada() {
    let spacing = 0.15;
    let filtered = distribute(&DistributionSettings {
        threshold: 0.4,
        ..settings(24531)
    })
    .unwrap();
    let spaced = distribute(&DistributionSettings {
        threshold: 0.4,
        min_spacing: spacing,
        ..settings(24531)
    })
    .unwrap();
    assert!(!spaced.is_empty() && spaced.len() < filtered.len());
    for (i, a) in spaced.iter().enumerate() {
        for b in &spaced[i + 1..] {
            assert!(a.attempt < b.attempt, "orden por intento");
            assert!((a.position - b.position).length() >= spacing);
        }
    }
    // Cada candidato descartado choca con una muestra aceptada antes que el.
    for candidate in filtered.iter().filter(|c| !spaced.contains(c)) {
        assert!(spaced.iter().any(|s| s.attempt < candidate.attempt
            && (s.position - candidate.position).length() < spacing));
    }
}

#[test]
fn rechaza_parametros_fuera_de_rango_antes_de_trabajar() {
    type Edit = fn(&mut DistributionSettings);
    let cases: [(Edit, &str); 14] = [
        // Primero los presupuestos: sin validacion, u32::MAX reservaria memoria enorme.
        (|s| s.attempts = MAX_ATTEMPTS + 1, "attempts"),
        (|s| s.attempts = u32::MAX, "attempts"),
        (|s| s.noise_scale = 0.0, "noise_scale"),
        (|s| s.noise_scale = -1.0, "noise_scale"),
        (|s| s.noise_scale = f64::NAN, "noise_scale"),
        (|s| s.noise_scale = f64::INFINITY, "noise_scale"),
        (|s| s.noise_scale = MAX_NOISE_SCALE * 2.0, "noise_scale"),
        (|s| s.threshold = -0.01, "threshold"),
        (|s| s.threshold = 1.01, "threshold"),
        (|s| s.threshold = f64::NAN, "threshold"),
        (|s| s.min_spacing = -0.01, "min_spacing"),
        (|s| s.min_spacing = 2.01, "min_spacing"),
        (|s| s.min_spacing = f64::NAN, "min_spacing"),
        (|s| s.min_spacing = f64::NEG_INFINITY, "min_spacing"),
    ];
    for (edit, field) in cases {
        let mut s = settings(1);
        edit(&mut s);
        let error = distribute(&s).expect_err(field);
        assert!(error.contains(field), "{s:?}: {error}");
    }
}

#[test]
fn acepta_los_extremos_documentados() {
    let ok = |edit: fn(&mut DistributionSettings)| {
        let mut s = settings(1);
        edit(&mut s);
        distribute(&s).unwrap()
    };
    assert!(
        ok(|s| s.attempts = 0).is_empty(),
        "cero intentos: salida vacia"
    );
    assert_eq!(
        ok(|s| s.attempts = MAX_ATTEMPTS).len(),
        MAX_ATTEMPTS as usize
    );
    assert!(!ok(|s| s.noise_scale = MAX_NOISE_SCALE).is_empty());
    assert!(ok(|s| s.threshold = 1.0).is_empty(), "la densidad es < 1");
    assert_eq!(
        ok(|s| s.min_spacing = 2.0).len(),
        1,
        "cuerda maxima: una muestra"
    );
}

#[test]
fn la_densidad_publica_rechaza_entradas_fuera_de_su_dominio() {
    // Hallazgo de revision: devolvia NaN con entradas finitas pero no representables.
    let rejected: [(f64, Vec3, &str); 10] = [
        (MAX_NOISE_SCALE, Vec3::splat(f64::MAX), "dominio"),
        (1.0, Vec3::new(MAX_NOISE_COORD * 2.0, 0.0, 0.0), "dominio"),
        (MAX_NOISE_SCALE, Vec3::new(0.0, 0.0, -1025.0), "dominio"),
        (1.0, Vec3::new(f64::NAN, 0.0, 0.0), "point"),
        (1.0, Vec3::new(0.0, f64::INFINITY, 0.0), "point"),
        (1.0, Vec3::new(0.0, 0.0, f64::NEG_INFINITY), "point"),
        (0.0, Vec3::X, "noise_scale"),
        (f64::NAN, Vec3::X, "noise_scale"),
        (f64::INFINITY, Vec3::X, "noise_scale"),
        (MAX_NOISE_SCALE * 2.0, Vec3::X, "noise_scale"),
    ];
    for (scale, point, field) in rejected {
        match density(7, scale, point) {
            Err(error) => assert!(error.contains(field), "{scale} {point:?}: {error}"),
            Ok(d) => panic!("{scale} {point:?} devolvio Ok({d})"),
        }
    }
    // Borde exacto del dominio (1024 * 1024 = 2^20) y punto lejos de la esfera unidad.
    for (scale, point) in [
        (MAX_NOISE_SCALE, Vec3::splat(-1024.0)),
        (1.0, Vec3::new(MAX_NOISE_COORD, -MAX_NOISE_COORD, 0.5)),
        (0.001, Vec3::splat(50.0)),
    ] {
        let d = density(7, scale, point).unwrap();
        assert!((0.0..1.0).contains(&d), "{scale} {point:?}: {d}");
    }
}
