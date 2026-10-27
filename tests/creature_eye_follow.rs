//! Mirada local del ojo (fase 1, nucleo puro): ajustes, envolvente, objetivo, estado temporal,
//! calibracion en reposo y pose en cache. Fixtures sinteticos; no necesita el asset real.

use umbral::visual::creature_eye_follow::*;

const DEG: f64 = std::f64::consts::PI / 180.0;

// --- Ajustes y envolvente ---

#[test]
fn settings_expose_only_a_validated_t95_and_the_limits_are_the_approved_ones() {
    assert_eq!(EYE_FOLLOW_MAX_YAW, 10.0 * DEG);
    assert_eq!(EYE_FOLLOW_MAX_PITCH, 8.0 * DEG);
    assert_eq!(EyeFollowSettings::default().t95_seconds(), 1.5);
    assert_eq!(
        EyeFollowSettings::new(1.5),
        Ok(EyeFollowSettings::default())
    );
    assert_eq!(
        (MIN_EYE_FOLLOW_SECONDS, MAX_EYE_FOLLOW_SECONDS),
        (0.01, 60.0)
    );
    for ok in [0.01, 60.0, 0.5] {
        assert_eq!(EyeFollowSettings::new(ok).unwrap().t95_seconds(), ok);
    }
    for bad in [0.0, 0.009, 60.01, -1.0, f64::NAN, f64::INFINITY] {
        assert!(EyeFollowSettings::new(bad).is_err(), "{bad}");
    }
}

#[test]
fn gaze_angles_always_stay_inside_the_approved_ellipse() {
    let inside = |g: GazeAngles| {
        (g.yaw() / EYE_FOLLOW_MAX_YAW).powi(2) + (g.pitch() / EYE_FOLLOW_MAX_PITCH).powi(2)
            <= 1.0 + 1e-12
    };
    // Dentro: sin cambios.
    let g = GazeAngles::clamped(4.0 * DEG, -3.0 * DEG);
    assert_eq!((g.yaw(), g.pitch()), (4.0 * DEG, -3.0 * DEG));
    // Ejes: el limite de cada eje.
    let g = GazeAngles::clamped(30.0 * DEG, 0.0);
    assert!((g.yaw() - 10.0 * DEG).abs() < 1e-15 && g.pitch() == 0.0);
    let g = GazeAngles::clamped(0.0, -30.0 * DEG);
    assert!((g.pitch() + 8.0 * DEG).abs() < 1e-15 && g.yaw() == 0.0);
    // Diagonal: no suma las dos amplitudes (no es un rectangulo); conserva la direccion.
    let g = GazeAngles::clamped(10.0 * DEG, 8.0 * DEG);
    assert!(inside(g));
    assert!(g.yaw() < 10.0 * DEG && g.pitch() < 8.0 * DEG);
    assert!((g.yaw() / g.pitch() - 10.0 / 8.0).abs() < 1e-12);
    // No finitos: neutral; resultados siempre finitos y dentro.
    for (y, p) in [
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
        (f64::NEG_INFINITY, 1.0),
        (1e300, -1e300),
    ] {
        let g = GazeAngles::clamped(y, p);
        assert!(
            g.yaw().is_finite() && g.pitch().is_finite() && inside(g),
            "{y} {p}"
        );
    }
    assert_eq!(GazeAngles::clamped(f64::NAN, 0.1), GazeAngles::NEUTRAL);
    assert!(GazeAngles::NEUTRAL.is_neutral());
    assert!(!GazeAngles::clamped(1e-9, 0.0).is_neutral());
    for i in 0..360 {
        let a = i as f64 * DEG;
        assert!(inside(GazeAngles::clamped(a.cos(), a.sin())));
    }
}

// --- Objetivo hacia la nave ---

use umbral::math::{Mat4, Vec3};

/// Eje optico calibrado del asset real (aprox.), no +Z puro.
fn asset_axis() -> Vec3 {
    Vec3::new(0.0362, 0.0169, 0.9992).normalized()
}

/// Punto a `distance` del pivote en la direccion del eje girado `yaw_obj`/`pitch_obj` (en el
/// marco del eje) y llevado al mundo con el yaw de la criatura.
fn target_at(
    pivot: Vec3,
    axis: Vec3,
    creature_yaw: f64,
    yaw_obj: f64,
    pitch_obj: f64,
    distance: f64,
) -> Vec3 {
    let u = Vec3::Y.cross(axis).normalized();
    let w = axis.cross(u);
    let dir = axis * (yaw_obj.cos() * pitch_obj.cos())
        + u * (yaw_obj.sin() * pitch_obj.cos())
        + w * pitch_obj.sin();
    pivot + Mat4::rotation_y(creature_yaw).transform_dir(dir) * distance
}

#[test]
fn a_target_on_the_optical_axis_gives_exact_neutral_for_any_creature_yaw() {
    let pivot = Vec3::new(10.0, -3.0, -900.0);
    for yaw in [0.0, 0.7, -2.5, std::f64::consts::PI] {
        for axis in [Vec3::Z, asset_axis()] {
            let target = target_at(pivot, axis, yaw, 0.0, 0.0, 250.0);
            let g = aim_gaze(pivot, axis, yaw, target);
            assert!(g.is_neutral(), "eje {axis:?} yaw {yaw}: {g:?}");
        }
    }
}

#[test]
fn the_gaze_follows_the_target_inside_the_ellipse_with_the_v02_sign_convention() {
    let pivot = Vec3::new(0.0, 0.0, -900.0);
    let axis = asset_axis();
    for creature_yaw in [0.0, 1.1] {
        // Dentro del alcance: apunta al objetivo.
        let g = aim_gaze(
            pivot,
            axis,
            creature_yaw,
            target_at(pivot, axis, creature_yaw, 5.0 * DEG, 0.0, 300.0),
        );
        assert!(
            (g.yaw() - 5.0 * DEG).abs() < 1e-9 && g.pitch().abs() < 1e-9,
            "{g:?}"
        );
        let g = aim_gaze(
            pivot,
            axis,
            creature_yaw,
            target_at(pivot, axis, creature_yaw, 0.0, -4.0 * DEG, 300.0),
        );
        assert!(
            (g.pitch() + 4.0 * DEG).abs() < 1e-9 && g.yaw().abs() < 1e-9,
            "{g:?}"
        );
        // Saturacion natural en el borde de la elipse.
        let g = aim_gaze(
            pivot,
            axis,
            creature_yaw,
            target_at(pivot, axis, creature_yaw, 40.0 * DEG, 0.0, 300.0),
        );
        assert!(
            (g.yaw() - 10.0 * DEG).abs() < 1e-9 && g.pitch().abs() < 1e-9,
            "{g:?}"
        );
    }
    // Yaw de la criatura pi/2: el +X del objeto es -Z del mundo.
    let g = aim_gaze(
        pivot,
        Vec3::Z,
        std::f64::consts::FRAC_PI_2,
        pivot + Vec3::new(0.3, 0.0, -1.0) * 100.0,
    );
    assert!(g.yaw() > 0.0, "{g:?}");
    // Arriba en el mundo es +Y del objeto: pitch positivo.
    let g = aim_gaze(
        pivot,
        Vec3::Z,
        0.0,
        pivot + Vec3::new(0.0, 0.3, 1.0) * 100.0,
    );
    assert!(g.pitch() > 0.0 && g.yaw().abs() < 1e-12, "{g:?}");
}

#[test]
fn behind_and_sideways_the_gaze_fades_continuously_back_to_rest() {
    assert_eq!(
        (EYE_FOLLOW_FADE_START, EYE_FOLLOW_FADE_END),
        (75.0 * DEG, 105.0 * DEG)
    );
    let pivot = Vec3::ZERO;
    let axis = Vec3::Z;
    // Detras: reposo exacto. A 90 grados de lado: la mitad del limite.
    assert!(aim_gaze(pivot, axis, 0.0, Vec3::new(0.0, 0.0, -50.0)).is_neutral());
    assert!(aim_gaze(pivot, axis, 0.0, Vec3::new(0.01, 0.0, -50.0)).is_neutral());
    let side = aim_gaze(pivot, axis, 0.0, Vec3::new(50.0, 0.0, 0.0));
    assert!((side.yaw() - 5.0 * DEG).abs() < 1e-9, "{side:?}");
    // Vueltas completas horizontal, vertical y oblicua: sin saltos (incluido detras y el polo).
    for (a, b) in [
        (Vec3::X, Vec3::Z),
        (Vec3::Y, Vec3::Z),
        (Vec3::new(1.0, 1.0, 0.0).normalized(), Vec3::Z),
    ] {
        let mut previous = None::<GazeAngles>;
        for i in 0..=3600 {
            let t = i as f64 * 0.1 * DEG;
            let g = aim_gaze(pivot, axis, 0.0, (a * t.sin() + b * t.cos()) * 100.0);
            if let Some(p) = previous {
                let jump = (g.yaw() - p.yaw()).hypot(g.pitch() - p.pitch());
                // Seguimiento 1:1 de 0.1 grados por paso, mas la pendiente maxima del desvanecido
                // (0.05 grados por paso): un salto real seria de varios grados.
                assert!(
                    jump < 0.15 * DEG,
                    "salto de {} grados en {}",
                    jump / DEG,
                    t / DEG
                );
            }
            previous = Some(g);
        }
    }
}

#[test]
fn degenerate_or_hostile_inputs_fall_back_to_neutral_and_outputs_stay_valid() {
    let p = Vec3::new(1.0, 2.0, 3.0);
    let nan = Vec3::new(f64::NAN, 0.0, 0.0);
    let inf = Vec3::new(0.0, f64::INFINITY, 0.0);
    for g in [
        aim_gaze(p, Vec3::Z, 0.0, p), // objetivo coincidente
        aim_gaze(nan, Vec3::Z, 0.0, p),
        aim_gaze(p, Vec3::Z, 0.0, inf),
        aim_gaze(p, Vec3::Z, f64::NAN, p + Vec3::X),
        aim_gaze(p, Vec3::ZERO, 0.0, p + Vec3::X), // eje nulo
        aim_gaze(p, Vec3::Y, 0.0, p + Vec3::X),    // eje vertical: sin marco
        aim_gaze(p, nan, 0.0, p + Vec3::X),
    ] {
        assert_eq!(g, GazeAngles::NEUTRAL);
    }
    // Longitudes extremas y subnormales: resultados validos (no NaN), en la elipse.
    let tiny = f64::MIN_POSITIVE / 8.0;
    for target in [
        p + Vec3::new(tiny, 0.0, tiny),
        Vec3::new(1e300, 0.0, 1e300),
        Vec3::new(-1e307, 1e307, 1e307),
        p + Vec3::new(0.0, 0.0, 1e-300),
    ] {
        let g = aim_gaze(p, Vec3::Z, 0.3, target);
        let e = (g.yaw() / EYE_FOLLOW_MAX_YAW).powi(2) + (g.pitch() / EYE_FOLLOW_MAX_PITCH).powi(2);
        assert!(
            g.yaw().is_finite() && g.pitch().is_finite() && e <= 1.0 + 1e-12,
            "{target:?}: {g:?}"
        );
    }
    // Direccion conservada con coordenadas enormes (sin desbordar al medir).
    let g = aim_gaze(Vec3::ZERO, Vec3::Z, 0.0, Vec3::new(1e300, 0.0, 1e301));
    assert!(g.yaw() > 0.0 && g.pitch() == 0.0, "{g:?}");
}

// --- Estado temporal ---

fn close(a: GazeAngles, b: GazeAngles, tolerance: f64) -> bool {
    (a.yaw() - b.yaw()).hypot(a.pitch() - b.pitch()) <= tolerance
}

#[test]
fn the_gaze_eases_toward_its_target_with_an_explicit_t95_in_real_seconds() {
    assert_eq!(EYE_FOLLOW_SNAP_RADIANS, 1e-6);
    let s = EyeFollowSettings::default();
    let mut eye = EyeFollow::new(s);
    assert_eq!(
        eye.current(),
        GazeAngles::NEUTRAL,
        "arranque determinista en reposo"
    );
    assert_eq!(eye.settings(), s);
    let target = GazeAngles::clamped(10.0 * DEG, 0.0);
    // t95: tras 1.5 s reales queda el 5 % del cambio.
    eye.update(target, 1.5);
    assert!(
        (eye.current().yaw() - 9.5 * DEG).abs() < 1e-12,
        "{:?}",
        eye.current()
    );
    // Troceado en N cuadros = un paso (independiente de los FPS), cuadros largos sin limite.
    let mut once = EyeFollow::new(s);
    once.update(target, 0.4);
    let mut split = EyeFollow::new(s);
    for _ in 0..40 {
        split.update(target, 0.01);
    }
    assert!(close(once.current(), split.current(), 1e-12));
    let mut long = EyeFollow::new(s);
    long.update(target, 0.25);
    let mut capped = EyeFollow::new(s);
    capped.update(target, 0.1);
    assert!(
        long.current().yaw() > capped.current().yaw(),
        "no se limita como el dt fisico"
    );
    // Llega exacto al objetivo (ajuste final de EYE_FOLLOW_SNAP_RADIANS), sin sobrepasarlo.
    let mut eye = EyeFollow::new(s);
    let mut previous = 0.0;
    for _ in 0..600 {
        eye.update(target, 1.0 / 60.0);
        let yaw = eye.current().yaw();
        assert!(
            yaw >= previous && yaw <= target.yaw(),
            "monotono sin sobrepaso"
        );
        previous = yaw;
    }
    assert_eq!(eye.current(), target);
    // Un paso enorme llega al objetivo y no lo pasa.
    let mut eye = EyeFollow::new(s);
    eye.update(GazeAngles::clamped(-3.0 * DEG, 5.0 * DEG), 1e9);
    assert_eq!(eye.current(), GazeAngles::clamped(-3.0 * DEG, 5.0 * DEG));
    // Volver al reposo termina en neutral exacto.
    eye.update(GazeAngles::NEUTRAL, 60.0);
    assert!(eye.current().is_neutral());
}

#[test]
fn invalid_elapsed_does_not_advance_and_snap_reset_are_exact() {
    let mut eye = EyeFollow::new(EyeFollowSettings::new(0.5).unwrap());
    let target = GazeAngles::clamped(-7.0 * DEG, 4.0 * DEG);
    eye.update(target, 0.1);
    let before = eye.current();
    assert!(!before.is_neutral());
    for elapsed in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        eye.update(GazeAngles::NEUTRAL, elapsed);
        assert_eq!(eye.current(), before, "{elapsed}");
    }
    eye.snap(target);
    assert_eq!(eye.current(), target);
    eye.reset();
    assert_eq!(eye.current(), GazeAngles::NEUTRAL);
    // Entre dos objetivos cualesquiera de la elipse el camino no sale de ella.
    let corners = [
        (10.0, 0.0),
        (-10.0, 0.0),
        (0.0, 8.0),
        (0.0, -8.0),
        (7.0, 5.6),
        (-7.0, -5.6),
    ];
    for &(ya, pa) in &corners {
        for &(yb, pb) in &corners {
            let mut eye = EyeFollow::new(EyeFollowSettings::default());
            eye.snap(GazeAngles::clamped(ya * DEG, pa * DEG));
            for _ in 0..200 {
                eye.update(GazeAngles::clamped(yb * DEG, pb * DEG), 0.013);
                let g = eye.current();
                let e = (g.yaw() / EYE_FOLLOW_MAX_YAW).powi(2)
                    + (g.pitch() / EYE_FOLLOW_MAX_PITCH).powi(2);
                assert!(e <= 1.0 + 1e-12, "{g:?}");
            }
        }
    }
}

// --- Calibracion en reposo ---

use umbral::{
    assets::obj::{Model, parse_obj},
    renderer::texture::Texture,
    visual::{
        staged_creature_reveal::{EyeRegion, StagedParts},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

/// Cupula esferica de radio 1 (apice +Z) en anillos de 10 grados, 16 segmentos, normales
/// radiales. Con `dome_region` las bandas 0-4 son ojo y las 5-7 cuerpo: la costura es el
/// anillo de 50 grados y el nucleo (anillo >= 3 desde la costura) los anillos de 0 a 20
/// grados (33 vertices sobre la esfera).
fn dome_source(rings: usize) -> String {
    let segments = 16;
    let mut s = String::from("vt 0.5 0.5\nv 0 0 1\nvn 0 0 1\n");
    let mut index = vec![vec![1usize]];
    let mut next = 2;
    for k in 1..=rings {
        let phi = (k as f64 * 10.0).to_radians();
        let mut ring = Vec::new();
        for j in 0..segments {
            let t = (j as f64 / segments as f64) * std::f64::consts::TAU;
            let (x, y, z) = (phi.sin() * t.cos(), phi.sin() * t.sin(), phi.cos());
            s += &format!("v {x} {y} {z}\nvn {x} {y} {z}\n");
            ring.push(next);
            next += 1;
        }
        index.push(ring);
    }
    let f = |a: usize, b: usize, c: usize| format!("f {a}/1/{a} {b}/1/{b} {c}/1/{c}\n");
    for j in 0..segments {
        s += &f(1, index[1][j], index[1][(j + 1) % segments]);
    }
    for k in 1..rings {
        for j in 0..segments {
            let (a, b) = (index[k][j], index[k][(j + 1) % segments]);
            let (c, d) = (index[k + 1][j], index[k + 1][(j + 1) % segments]);
            s += &f(a, c, d);
            s += &f(a, d, b);
        }
    }
    s
}

fn dome() -> Model {
    parse_obj(&dome_source(8)).unwrap()
}

fn dome_region() -> EyeRegion {
    EyeRegion::new(Vec3::Z, 0.85).unwrap()
}

#[test]
fn calibration_reuses_the_stages_identity_and_the_v02_weights() {
    let model = dome();
    let skin = EyeSkin::calibrate(&model, &dome_region()).unwrap();
    assert_eq!(EYE_RIGID_RING, 3);
    // Identidad: las mismas caras y en el mismo orden que la particion de STAGES.
    let parts = StagedParts::split(&model, &dome_region()).unwrap();
    let eye: Vec<usize> = (0..model.faces.len())
        .filter(|&i| skin.eye_faces()[i])
        .collect();
    assert_eq!(eye.len(), parts.eye_faces());
    for (f, &i) in parts.eye().faces.iter().zip(&eye) {
        assert_eq!(f.vertex_indices, model.faces[i].vertex_indices);
    }
    // Costura (anillo de 50 grados) y todo el cuerpo con peso 0 exacto; nucleo con 1 exacto.
    assert_eq!((skin.shared_vertices(), skin.core_vertices()), (16, 33));
    let w = skin.weights();
    assert_eq!(w.len(), model.vertices.len());
    let body: Vec<usize> = (0..model.faces.len())
        .filter(|&i| !skin.eye_faces()[i])
        .flat_map(|i| model.faces[i].vertex_indices)
        .collect();
    assert!(body.iter().all(|&v| w[v] == 0.0), "cuerpo y costura en 0");
    assert_eq!(w.iter().filter(|&&x| x == 1.0).count(), 33, "nucleo exacto");
    // Transicion continua y monotona hacia el apice (anillos de 40 y 30 grados).
    // Indices en base 0 de `Model::vertices` (el apice es 0; el anillo k empieza en 1 + (k-1)*16).
    let ring = |k: usize| 1 + (k - 1) * 16;
    let (w40, w30) = (w[ring(4)], w[ring(3)]);
    assert!(0.0 < w40 && w40 < w30 && w30 < 1.0, "{w40} {w30}");
    assert!(
        (0..16).all(|j| (w[ring(4) + j] - w40).abs() < 1e-12),
        "simetrico"
    );
    // Solo se mueven los vertices con peso.
    assert_eq!(skin.moving_vertices().len(), 33 + 32);
    assert!(skin.moving_vertices().iter().all(|&v| w[v] > 0.0));
    // Pivote = centro de la esfera de la cupula; eje optico de la superficie (+Z aqui).
    assert!(skin.pivot().length() < 1e-9, "{:?}", skin.pivot());
    assert!(
        (skin.optical_axis() - Vec3::Z).length() < 1e-9,
        "{:?}",
        skin.optical_axis()
    );
}

#[test]
fn the_posed_bounds_are_a_conservative_envelope_of_the_moving_vertices() {
    let model = dome();
    let skin = EyeSkin::calibrate(&model, &dome_region()).unwrap();
    let c = Vec3::from_array(model.bounds_center);
    let r = skin.posed_bounds_radius();
    assert!(r >= model.bounds_radius);
    // Cota: |pivote - centro| + |v - pivote| para los vertices que se mueven (girar alrededor
    // del pivote conserva |v - pivote|).
    let bound = skin
        .moving_vertices()
        .iter()
        .map(|&v| {
            (skin.pivot() - c).length()
                + (Vec3::from_array(model.vertices[v]) - skin.pivot()).length()
        })
        .fold(model.bounds_radius, f64::max);
    assert!((r - bound).abs() < 1e-12, "{r} {bound}");
    assert_eq!(skin.fits_bind_sphere(), r <= model.bounds_radius);
}

#[test]
fn calibration_fails_closed_for_models_it_cannot_calibrate() {
    let model = dome();
    let check = |result: Result<EyeSkin, String>, needle: &str| {
        let error = result.expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    };
    // Region vacia o total.
    check(
        EyeSkin::calibrate(&model, &EyeRegion::new(Vec3::splat(9.0), 0.1).unwrap()),
        "ninguna cara",
    );
    check(
        EyeSkin::calibrate(&model, &EyeRegion::new(Vec3::Z, 50.0).unwrap()),
        "todas las caras",
    );
    // Ojo poco profundo: ningun vertice a 3 anillos de la costura.
    check(
        EyeSkin::calibrate(&model, &EyeRegion::new(Vec3::Z, 0.4).unwrap()),
        "nucleo",
    );
    // Ojo sin costura: la cupula entera es ojo y el cuerpo es un triangulo aparte.
    let island = dome_source(5) + "v 5 5 5\nv 6 5 5\nv 5 6 5\nf 82/1/1 83/1/1 84/1/1\n";
    check(
        EyeSkin::calibrate(&parse_obj(&island).unwrap(), &dome_region()),
        "costura",
    );
    // Datos de entrada invalidos.
    let mut bad = dome();
    bad.vertices[3][0] = f64::NAN;
    check(EyeSkin::calibrate(&bad, &dome_region()), "no finit");
    let mut bad = dome();
    bad.faces[0].vertex_indices[0] = 9999;
    check(EyeSkin::calibrate(&bad, &dome_region()), "fuera de rango");
}

#[test]
fn a_flat_core_cannot_fit_a_pivot_and_is_rejected() {
    // Disco plano z = 0 de anillos concentricos: el nucleo es coplanar.
    let segments = 12;
    let mut s = String::from("vt 0 0\nvn 0 0 1\nv 0 0 0\n");
    for k in 1..=8 {
        for j in 0..segments {
            let t = j as f64 / segments as f64 * std::f64::consts::TAU;
            s += &format!("v {} {} 0\n", k as f64 * t.cos(), k as f64 * t.sin());
        }
    }
    let id = |k: usize, j: usize| {
        if k == 0 {
            1
        } else {
            2 + (k - 1) * segments + j % segments
        }
    };
    for j in 0..segments {
        s += &format!("f {}/1/1 {}/1/1 {}/1/1\n", id(0, 0), id(1, j), id(1, j + 1));
    }
    for k in 1..8 {
        for j in 0..segments {
            let (a, b, c, d) = (id(k, j), id(k, j + 1), id(k + 1, j), id(k + 1, j + 1));
            s += &format!("f {a}/1/1 {c}/1/1 {d}/1/1\nf {a}/1/1 {d}/1/1 {b}/1/1\n");
        }
    }
    let flat = parse_obj(&s).unwrap();
    let error =
        EyeSkin::calibrate(&flat, &EyeRegion::new(Vec3::ZERO, 5.5).unwrap()).expect_err("plano");
    assert!(error.contains("pivote"), "{error}");
}

#[test]
fn the_skin_aims_from_the_world_pivot_of_the_creature() {
    let skin = EyeSkin::calibrate(&dome(), &dome_region()).unwrap();
    let placement = CreaturePlacement::new(Vec3::new(5.0, 0.0, -300.0), 40.0, 0.6).unwrap();
    let texture = Texture::from_rgb8(1, 1, vec![9, 9, 9]).unwrap();
    let creature = StaticCreature::new(dome(), texture, placement).unwrap();
    let pivot_world = creature.transform().matrix().transform_point(skin.pivot());
    let axis_world = Mat4::rotation_y(0.6).transform_dir(skin.optical_axis());
    assert!(
        skin.aim(&creature, pivot_world + axis_world * 500.0)
            .is_neutral()
    );
    let target =
        pivot_world + Mat4::rotation_y(0.6).transform_dir(Vec3::new(0.05, 0.0, 1.0)) * 500.0;
    let g = skin.aim(&creature, target);
    assert_eq!(g, aim_gaze(pivot_world, skin.optical_axis(), 0.6, target));
    assert!(g.yaw() > 0.0);
}

// --- Pose en cache ---

fn posed_cache() -> (Model, EyeSkin, EyePoseCache) {
    let bind = dome();
    let skin = EyeSkin::calibrate(&bind, &dome_region()).unwrap();
    let cache = EyePoseCache::new(&bind, &skin).unwrap();
    (bind, skin, cache)
}

#[test]
fn the_cache_holds_only_the_eye_faces_with_private_corner_normals() {
    let (bind, skin, cache) = posed_cache();
    let eye = cache.eye_model();
    let eye_idx: Vec<usize> = (0..bind.faces.len())
        .filter(|&i| skin.eye_faces()[i])
        .collect();
    assert_eq!(eye.faces.len(), eye_idx.len());
    assert_eq!(eye.vertices, bind.vertices, "reposo exacto");
    assert_eq!(eye.uvs, bind.uvs);
    assert_eq!(eye.materials, bind.materials);
    assert_eq!(
        (eye.bounds_center, eye.bounds_radius),
        (bind.bounds_center, bind.bounds_radius)
    );
    assert_eq!(
        &eye.normals[..bind.normals.len()],
        &bind.normals[..],
        "normales originales intactas"
    );
    for (f, &i) in eye.faces.iter().zip(&eye_idx) {
        let g = &bind.faces[i];
        assert_eq!(
            (f.vertex_indices, f.uv_indices, &f.material),
            (g.vertex_indices, g.uv_indices, &g.material)
        );
        for (slot, (&copy, &source)) in f
            .normal_indices
            .unwrap()
            .iter()
            .zip(&g.normal_indices.unwrap())
            .enumerate()
        {
            assert!(copy >= bind.normals.len(), "esquina {slot}: copia privada");
            assert_eq!(
                eye.normals[copy], bind.normals[source],
                "mismo valor en reposo"
            );
        }
    }
    assert_eq!(cache.posed(), GazeAngles::NEUTRAL);
}

#[test]
fn poses_are_built_from_rest_in_place_without_accumulating() {
    let (bind, skin, mut cache) = posed_cache();
    let rest = cache.eye_model().vertices.clone();
    let rest_normals = cache.eye_model().normals.clone();
    let (vertices_ptr, normals_ptr) = (
        cache.eye_model().vertices.as_ptr(),
        cache.eye_model().normals.as_ptr(),
    );
    let a = GazeAngles::clamped(10.0 * DEG, 0.0);
    let b = GazeAngles::clamped(-4.0 * DEG, 6.0 * DEG);
    cache.pose(&bind, &skin, a).unwrap();
    assert_eq!(cache.posed(), a);
    cache.pose(&bind, &skin, b).unwrap();
    // Igual que posar b desde un cache nuevo: no se acumula sobre a.
    let mut fresh = EyePoseCache::new(&bind, &skin).unwrap();
    fresh.pose(&bind, &skin, b).unwrap();
    assert_eq!(cache.eye_model().vertices, fresh.eye_model().vertices);
    assert_eq!(cache.eye_model().normals, fresh.eye_model().normals);
    // En sitio: mismos buffers.
    assert_eq!(cache.eye_model().vertices.as_ptr(), vertices_ptr);
    assert_eq!(cache.eye_model().normals.as_ptr(), normals_ptr);
    // Volver al reposo: exacto, bit a bit.
    cache.pose(&bind, &skin, GazeAngles::NEUTRAL).unwrap();
    assert_eq!(cache.eye_model().vertices, rest);
    assert_eq!(cache.eye_model().normals, rest_normals);
}

#[test]
fn only_weighted_vertices_move_and_the_rigid_core_turns_about_the_pivot() {
    let (bind, skin, mut cache) = posed_cache();
    let (yaw, pitch) = (8.0 * DEG, -3.0 * DEG);
    cache
        .pose(&bind, &skin, GazeAngles::clamped(yaw, pitch))
        .unwrap();
    let posed = &cache.eye_model().vertices;
    let w = skin.weights();
    // Peso 0 (costura y cuerpo): exactos; las normales originales no cambian.
    for i in (0..bind.vertices.len()).filter(|&i| w[i] == 0.0) {
        assert_eq!(posed[i], bind.vertices[i], "vertice {i}");
    }
    assert_eq!(
        &cache.eye_model().normals[..bind.normals.len()],
        &bind.normals[..]
    );
    // Nucleo (peso 1): giro rigido pitch y luego yaw alrededor del pivote (convencion de V02).
    let rotation = Mat4::rotation_y(yaw) * Mat4::rotation_x(-pitch);
    let p = skin.pivot();
    for i in (0..bind.vertices.len()).filter(|&i| w[i] == 1.0) {
        let expected = p + rotation.transform_dir(Vec3::from_array(bind.vertices[i]) - p);
        assert!(
            (Vec3::from_array(posed[i]) - expected).length() < 1e-12,
            "nucleo {i}"
        );
    }
    // Transicion: gira menos que el nucleo y mas que la costura.
    let ring40 = 1 + 3 * 16;
    let moved =
        (Vec3::from_array(posed[ring40]) - Vec3::from_array(bind.vertices[ring40])).length();
    let full = (p + rotation.transform_dir(Vec3::from_array(bind.vertices[ring40]) - p)
        - Vec3::from_array(bind.vertices[ring40]))
    .length();
    assert!(moved > 0.0 && moved < full, "{moved} {full}");
    // Yaw positivo lleva el apice (el centro del ojo) hacia +X.
    assert!(posed[0][0] > bind.vertices[0][0]);
    // La cota de la calibracion envuelve la pose.
    let c = Vec3::from_array(bind.bounds_center);
    assert!(
        posed
            .iter()
            .all(|&q| (Vec3::from_array(q) - c).length() <= skin.posed_bounds_radius() + 1e-12)
    );
}

#[test]
fn mismatched_inputs_are_rejected_without_touching_the_cache() {
    let (bind, skin, mut cache) = posed_cache();
    let other = dome_source(7);
    let other = parse_obj(&other).unwrap();
    // Un skin calibrado para otro modelo no sirve para construir.
    assert!(EyePoseCache::new(&other, &skin).is_err());
    let a = GazeAngles::clamped(5.0 * DEG, 2.0 * DEG);
    cache.pose(&bind, &skin, a).unwrap();
    let before = (
        cache.eye_model().vertices.clone(),
        cache.eye_model().normals.clone(),
    );
    assert!(cache.pose(&other, &skin, GazeAngles::NEUTRAL).is_err());
    let other_skin = EyeSkin::calibrate(&other, &EyeRegion::new(Vec3::Z, 0.85).unwrap()).unwrap();
    assert!(cache.pose(&bind, &other_skin, GazeAngles::NEUTRAL).is_err());
    assert_eq!(
        (
            cache.eye_model().vertices.clone(),
            cache.eye_model().normals.clone()
        ),
        before
    );
    assert_eq!(cache.posed(), a);
}

// --- Identidad del modelo y del skin, indices, normales y bounds ---

/// Estado observable del cache, para comprobar que un rechazo no lo toca.
fn cache_state(cache: &EyePoseCache) -> (Vec<[f64; 3]>, Vec<[f64; 3]>, GazeAngles) {
    let eye = cache.eye_model();
    (eye.vertices.clone(), eye.normals.clone(), cache.posed())
}

/// Edicion en sitio de un modelo, con su nombre para los mensajes.
type ModelEdit = (&'static str, fn(&mut Model));

#[test]
fn a_different_bind_or_skin_is_rejected_even_with_the_same_sizes() {
    let (mut bind, skin, mut cache) = posed_cache();
    let a = GazeAngles::clamped(6.0 * DEG, -2.0 * DEG);
    cache.pose(&bind, &skin, a).unwrap();
    let before = cache_state(&cache);
    // Otro modelo con los mismos tamanos (un vertice desplazado).
    let mut other = dome();
    other.vertices[0][0] += 0.1;
    assert!(EyePoseCache::new(&other, &skin).is_err());
    assert!(cache.pose(&other, &skin, a).is_err());
    assert_eq!(cache_state(&cache), before);
    // Otra calibracion del mismo modelo, con otras caras de ojo.
    let wide = EyeSkin::calibrate(&bind, &EyeRegion::new(Vec3::Z, 0.95).unwrap()).unwrap();
    let count = |s: &EyeSkin| s.eye_faces().iter().filter(|&&e| e).count();
    assert_eq!((count(&skin), count(&wide)), (144, 176));
    assert!(cache.pose(&bind, &wide, a).is_err());
    assert_eq!(cache_state(&cache), before);
    // El mismo modelo editado en sitio: todo su contenido cuenta para la identidad.
    let edits: [ModelEdit; 11] = [
        ("vertice", |m| m.vertices[5][1] += 1e-9),
        ("topologia", |m| m.faces[0].vertex_indices.swap(1, 2)),
        ("normal", |m| m.normals[7][2] *= 0.5),
        ("indices de normal", |m| {
            m.faces[2].normal_indices = Some([1, 1, 1])
        }),
        ("uv", |m| m.uvs[0][0] = 0.25),
        ("indices de uv", |m| m.faces[2].uv_indices = None),
        ("material de cara", |m| {
            m.faces[1].material = Some("otro".into())
        }),
        ("materiales", |m| {
            m.materials.insert("otro".into(), [1.0, 0.0, 0.0]);
        }),
        ("bibliotecas", |m| {
            m.material_libraries.push("otro.mtl".into())
        }),
        ("centro", |m| m.bounds_center[2] += 0.5),
        ("radio", |m| m.bounds_radius += 0.5),
    ];
    for (what, edit) in edits {
        let pointer = bind.vertices.as_ptr();
        edit(&mut bind);
        assert_eq!(bind.vertices.as_ptr(), pointer, "{what}: en sitio");
        assert!(EyePoseCache::new(&bind, &skin).is_err(), "{what}: new");
        assert!(
            cache.pose(&bind, &skin, GazeAngles::NEUTRAL).is_err(),
            "{what}: pose"
        );
        assert_eq!(cache_state(&cache), before, "{what}");
        bind = dome();
    }
    // Contrato: cuenta el contenido, no la direccion. Otra lectura del mismo OBJ, un clon del
    // skin o una recalibracion identica son la misma identidad.
    let same = dome();
    assert!(EyePoseCache::new(&same, &skin.clone()).is_ok());
    cache.pose(&same, &skin.clone(), a).unwrap();
    let again = EyeSkin::calibrate(&same, &dome_region()).unwrap();
    cache.pose(&same, &again, a).unwrap();
    assert_eq!(cache_state(&cache), before);
}

#[test]
fn an_out_of_range_eye_vertex_is_rejected_without_panicking_or_writing() {
    let (bind, skin, mut cache) = posed_cache();
    let first_eye = skin.eye_faces().iter().position(|&e| e).unwrap();
    let mut broken = dome();
    broken.faces[first_eye].vertex_indices[0] = broken.vertices.len();
    // Construir con la cara rota (mismos tamanos): Err, sin grabar esquinas invalidas.
    assert!(EyePoseCache::new(&broken, &skin).is_err());
    // Posar un cache valido con el modelo invalidado: Err antes de escribir, sin panico.
    let a = GazeAngles::clamped(-5.0 * DEG, 3.0 * DEG);
    cache.pose(&bind, &skin, a).unwrap();
    let before = cache_state(&cache);
    assert!(
        cache
            .pose(&broken, &skin, GazeAngles::clamped(9.0 * DEG, 0.0))
            .is_err()
    );
    assert_eq!(cache_state(&cache), before);
}

#[test]
fn non_finite_or_unrepresentable_normals_fail_closed() {
    let bad_normals = [
        [f64::NAN, 0.0, 1.0],
        [0.0, f64::INFINITY, 1.0],
        // Finita, pero girarla 10 grados desborda a infinito.
        [f64::MAX, 0.0, f64::MAX],
    ];
    // Calibrar directamente sobre una normal mala del ojo: Err.
    for bad in bad_normals {
        let mut model = dome();
        model.normals[0] = bad;
        let error = EyeSkin::calibrate(&model, &dome_region()).expect_err("normal mala");
        assert!(error.contains("normal"), "{bad:?}: {error}");
    }
    // Una normal mala despues de calibrar: ni new ni pose la usan y el cache no cambia.
    let (bind, skin, mut cache) = posed_cache();
    let a = GazeAngles::clamped(10.0 * DEG, 0.0);
    cache.pose(&bind, &skin, a).unwrap();
    let before = cache_state(&cache);
    for bad in bad_normals {
        let mut model = dome();
        model.normals[0] = bad;
        assert!(EyePoseCache::new(&model, &skin).is_err(), "{bad:?}");
        assert!(cache.pose(&model, &skin, a).is_err(), "{bad:?}");
        assert_eq!(cache_state(&cache), before, "{bad:?}");
    }
    // Normales grandes pero representables: se aceptan tal cual (no se normalizan) y la pose
    // conserva su longitud; todas las salidas son finitas.
    let mut big = dome();
    big.normals[0] = [0.0, 0.0, 1e100];
    let skin = EyeSkin::calibrate(&big, &dome_region()).unwrap();
    let mut cache = EyePoseCache::new(&big, &skin).unwrap();
    cache.pose(&big, &skin, a).unwrap();
    let eye = cache.eye_model();
    assert!(
        eye.vertices
            .iter()
            .chain(&eye.normals)
            .flatten()
            .all(|c| c.is_finite())
    );
    assert_eq!(&eye.normals[..big.normals.len()], &big.normals[..]);
    let copy = eye.faces[0].normal_indices.unwrap()[0];
    assert_eq!(big.faces[0].normal_indices.unwrap()[0], 0);
    let length = Vec3::from_array(eye.normals[copy]).length();
    assert!((length / 1e100 - 1.0).abs() < 1e-12, "{length}");
    assert_ne!(eye.normals[copy], big.normals[0], "girada");
}

#[test]
fn bounds_without_integrity_fail_closed() {
    // Metadatos de bounds sin integridad, o tan lejos que la cota desborda: Err.
    let bad: [([f64; 3], f64); 6] = [
        ([f64::NAN, 0.0, 0.0], 1.0),
        ([0.0, f64::INFINITY, 0.0], 1.0),
        ([0.0, 0.0, 0.5], f64::NAN),
        ([0.0, 0.0, 0.5], f64::INFINITY),
        ([0.0, 0.0, 0.5], -1.0),
        ([f64::MAX, 0.0, 0.0], 1.0),
    ];
    for (center, radius) in bad {
        let mut model = dome();
        (model.bounds_center, model.bounds_radius) = (center, radius);
        let error = EyeSkin::calibrate(&model, &dome_region()).expect_err("bounds");
        assert!(error.contains("bounds"), "{center:?} {radius}: {error}");
    }
}

#[test]
fn the_posed_envelope_covers_all_the_geometry_even_with_an_insufficient_sphere() {
    // Esfera declarada insuficiente (un cuerpo lejano con los bounds viejos de la cupula, o
    // radio 0): la cota no se fia de ella y envuelve toda la geometria en cualquier pose.
    let mut far =
        parse_obj(&(dome_source(8) + "v 5 5 5\nv 6 5 5\nv 5 6 5\nf 130/1/1 131/1/1 132/1/1\n"))
            .unwrap();
    let old = dome();
    (far.bounds_center, far.bounds_radius) = (old.bounds_center, old.bounds_radius);
    let mut zero = dome();
    zero.bounds_radius = 0.0;
    for model in [far, zero] {
        let skin = EyeSkin::calibrate(&model, &dome_region()).unwrap();
        let (c, r) = (
            Vec3::from_array(model.bounds_center),
            skin.posed_bounds_radius(),
        );
        assert!(r.is_finite() && !skin.fits_bind_sphere(), "{r}");
        let mut cache = EyePoseCache::new(&model, &skin).unwrap();
        for (yaw, pitch) in [
            (0.0, 0.0),
            (10.0, 0.0),
            (-10.0, 0.0),
            (0.0, 8.0),
            (7.0, -5.6),
        ] {
            let g = GazeAngles::clamped(yaw * DEG, pitch * DEG);
            cache.pose(&model, &skin, g).unwrap();
            // El cache tiene todos los vertices: los del ojo posados y el resto en reposo.
            assert!(
                cache
                    .eye_model()
                    .vertices
                    .iter()
                    .all(|&q| (Vec3::from_array(q) - c).length() <= r + 1e-12),
                "{yaw} {pitch}: {r}"
            );
        }
    }
}
