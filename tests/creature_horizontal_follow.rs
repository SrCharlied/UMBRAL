//! Seguimiento corporal horizontal (candidata 02): nucleo puro, transform rigido, `World`,
//! render, panel y binario. Fixtures sinteticos salvo la prueba `#[ignore]` del asset real
//! (rutas por entorno).

use std::f64::consts::PI;
use umbral::{
    math::{Mat4, Vec3},
    visual::creature_horizontal_follow::*,
};

const DEG: f64 = PI / 180.0;

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// Diferencia angular por el arco corto (para comparar angulos envueltos).
fn arc(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(2.0 * PI);
    if d > PI { d - 2.0 * PI } else { d }
}

/// Punto en el plano horizontal del centro, a `azimuth` del frente de reposo de una criatura
/// con giro `creature_yaw` (azimut positivo hacia +X del OBJ), mas una altura `up`.
fn point_at(center: Vec3, creature_yaw: f64, azimuth: f64, length: f64, up: f64) -> Vec3 {
    center + Mat4::rotation_y(creature_yaw + azimuth).transform_dir(Vec3::Z) * length + Vec3::Y * up
}

// --- H1: flags, angulos envueltos y azimut ---

#[test]
fn flags_need_the_comparison_and_exclude_legacy_and_the_distant_turn() {
    assert_eq!(
        validate_horizontal_follow_flags(false, false, false, false),
        Ok(())
    );
    assert_eq!(
        validate_horizontal_follow_flags(false, false, true, true),
        Ok(())
    );
    assert_eq!(
        validate_horizontal_follow_flags(true, true, false, false),
        Ok(())
    );
    assert_eq!(
        validate_horizontal_follow_flags(true, false, false, false),
        Err("--creature-horizontal-follow necesita --creature-eye-compare".into())
    );
    assert_eq!(
        validate_horizontal_follow_flags(true, true, true, false),
        Err(
            "--creature-horizontal-follow y --creature-distant-turn son incompatibles: elige uno"
                .into()
        )
    );
    for (compare, distant) in [(true, false), (false, true), (true, true), (false, false)] {
        assert_eq!(
            validate_horizontal_follow_flags(true, compare, distant, true),
            Err("--legacy no admite --creature-horizontal-follow".into())
        );
    }
}

#[test]
fn angles_wrap_into_the_half_open_interval_and_stay_finite() {
    assert_eq!(wrap_angle(0.0), 0.0);
    assert_eq!(wrap_angle(PI), PI);
    assert_eq!(wrap_angle(-PI), PI);
    assert!(close(wrap_angle(3.0 * PI), PI, 1e-12));
    assert!(close(wrap_angle(181.0 * DEG), -179.0 * DEG, 1e-12));
    assert!(close(wrap_angle(-181.0 * DEG), 179.0 * DEG, 1e-12));
    assert!(close(wrap_angle(10.0 * 2.0 * PI + 0.3), 0.3, 1e-9));
    assert!(close(wrap_angle(-7.0 * 2.0 * PI - 0.3), -0.3, 1e-9));
    for a in [-1e6, -4.0, -PI, 0.1, 2.5, 7.0, 1e6] {
        let w = wrap_angle(a);
        assert!(w > -PI && w <= PI, "{a} -> {w}");
        assert!(close(arc(w, a), 0.0, 1e-6), "{a}");
    }
    for a in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(wrap_angle(a), 0.0, "{a}");
    }
}

#[test]
fn the_target_azimuth_covers_every_side_in_the_rest_frame_and_ignores_height() {
    let center = Vec3::new(30.0, -4.0, -900.0);
    let creature_yaw = 0.9;
    for degrees in [
        0.0, 25.0, 60.0, 90.0, 135.0, 179.0, 180.0, -179.0, -90.0, -45.0,
    ] {
        let a = degrees * DEG;
        for up in [0.0, 500.0, -300.0] {
            let azimuth = target_azimuth(
                center,
                creature_yaw,
                point_at(center, creature_yaw, a, 640.0, up),
            )
            .expect("con direccion");
            assert!(
                close(arc(azimuth, a), 0.0, 1e-12),
                "{degrees} {up}: {azimuth}"
            );
            assert!(azimuth > -PI && azimuth <= PI);
        }
    }
    // Sin direccion XZ (sobre/bajo el centro, coincidente) o no finito: `None`.
    for target in [
        center,
        center + Vec3::Y * 100.0,
        center - Vec3::Y * 1e6,
        Vec3::new(f64::NAN, 0.0, 0.0),
        Vec3::new(0.0, f64::INFINITY, 0.0),
    ] {
        assert_eq!(
            target_azimuth(center, creature_yaw, target),
            None,
            "{target:?}"
        );
    }
    assert_eq!(target_azimuth(center, f64::NAN, Vec3::ZERO), None);
    // Distancias diminutas o enormes no desbordan.
    let tiny = target_azimuth(Vec3::ZERO, 0.0, Vec3::new(1e-300, 0.0, 1e-300)).unwrap();
    let huge = target_azimuth(Vec3::ZERO, 0.0, Vec3::new(1e300, 1e300, 1e300)).unwrap();
    assert!(close(tiny, 45.0 * DEG, 1e-12) && tiny == huge);
}

// --- H2: dinamica circular ---

const DT: f64 = 1.0 / 60.0;

/// Avanza `seconds` a 60 Hz hacia `azimuth` fijo; comprueba en cada paso la rapidez maxima,
/// la finitud y que el estado siga envuelto. Devuelve el recorrido desenvuelto (suma de pasos
/// con signo).
fn run_fixed(follow: &mut HorizontalFollow, azimuth: Option<f64>, seconds: f64) -> f64 {
    let mut travel = 0.0;
    for _ in 0..(seconds / DT).round() as usize {
        let before = follow.yaw();
        follow.update(azimuth, DT);
        let step = arc(follow.yaw(), before);
        assert!(
            step.abs() <= HORIZONTAL_FOLLOW_MAX_SPEED * DT + 1e-12,
            "{step}"
        );
        assert!(close(follow.speed(), step.abs() / DT, 1e-9));
        assert!(follow.yaw().is_finite() && follow.yaw() > -PI && follow.yaw() <= PI);
        travel += step;
    }
    travel
}

fn settled_at(yaw: f64) -> HorizontalFollow {
    let mut follow = HorizontalFollow::new();
    follow.update(Some(yaw), 1000.0);
    assert_eq!(follow.yaw(), wrap_angle(yaw));
    follow
}

#[test]
fn small_changes_ease_with_t95_and_large_ones_are_capped_at_the_maximum_speed() {
    let follow = HorizontalFollow::new();
    assert!(follow.is_active());
    assert_eq!(
        (follow.yaw(), follow.goal(), follow.speed()),
        (0.0, None, 0.0)
    );
    assert_eq!(HORIZONTAL_FOLLOW_T95_SECONDS, 4.0);
    assert!(close(HORIZONTAL_FOLLOW_MAX_SPEED, 18.0 * DEG, 1e-15));
    // Cambio chico (10 grados, bajo el tope): el 95 % en 4 s, sin pasarse.
    let mut small = follow;
    let travel = run_fixed(&mut small, Some(10.0 * DEG), 4.0);
    assert!(close(1.0 - travel / (10.0 * DEG), 0.05, 1e-9), "{travel}");
    assert_eq!(small.goal(), Some(10.0 * DEG));
    // Cambio grande (150 grados): rapidez exacta en el tope hasta quedar a vmax / k.
    let mut large = follow;
    let k = 20f64.ln() / HORIZONTAL_FOLLOW_T95_SECONDS;
    let knee = HORIZONTAL_FOLLOW_MAX_SPEED / k;
    let linear = (150.0 * DEG - knee) / HORIZONTAL_FOLLOW_MAX_SPEED;
    run_fixed(&mut large, Some(150.0 * DEG), 1.0);
    assert!(close(large.speed(), HORIZONTAL_FOLLOW_MAX_SPEED, 1e-9));
    assert!(close(large.yaw(), HORIZONTAL_FOLLOW_MAX_SPEED * 1.0, 1e-9));
    // Independiente del troceo, con el codo de las dos fases dentro de un paso.
    for total in [linear - 0.3, linear + 0.7, 30.0] {
        let mut once = follow;
        once.update(Some(150.0 * DEG), total);
        let mut stepped = follow;
        let n = (total / 0.05).floor() as usize;
        for _ in 0..n {
            stepped.update(Some(150.0 * DEG), 0.05);
        }
        stepped.update(Some(150.0 * DEG), total - n as f64 * 0.05);
        assert!(close(once.yaw(), stepped.yaw(), 1e-12), "{total}");
        assert!(once.yaw() <= 150.0 * DEG);
    }
    // Llega exacto y se queda; elapsed invalido no avanza.
    let mut done = follow;
    done.update(Some(150.0 * DEG), 1000.0);
    assert_eq!(done.yaw(), 150.0 * DEG);
    for dt in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut same = small;
        same.update(Some(-90.0 * DEG), dt);
        assert_eq!(same, small, "{dt}");
    }
}

#[test]
fn moving_farther_to_the_side_keeps_turning_and_never_returns_to_the_front() {
    // Regresiones del diagnostico de la candidata 01: 25 -> 60 siguen cambiando y 60 -> 90
    // nunca vuelve hacia el frente mientras se mantenga ese objetivo.
    let mut follow = settled_at(25.0 * DEG);
    let travel = run_fixed(&mut follow, Some(60.0 * DEG), 1.0);
    assert!(travel > 10.0 * DEG, "{travel}");
    let mut previous = follow.yaw();
    follow = settled_at(60.0 * DEG);
    for _ in 0..600 {
        follow.update(Some(90.0 * DEG), DT);
        assert!(follow.yaw() >= previous && follow.yaw() <= 90.0 * DEG);
        previous = follow.yaw();
    }
    assert!(close(follow.yaw(), 90.0 * DEG, 0.05 * 30.0 * DEG));
    // Izquierda, derecha y detras se alcanzan y se sostienen.
    for degrees in [-90.0, 90.0, 180.0, -150.0, 135.0] {
        let mut f = HorizontalFollow::new();
        run_fixed(&mut f, Some(degrees * DEG), 60.0);
        assert!(close(arc(f.yaw(), degrees * DEG), 0.0, 1e-12), "{degrees}");
        run_fixed(&mut f, Some(degrees * DEG), 5.0);
        assert!(close(arc(f.yaw(), degrees * DEG), 0.0, 1e-12), "{degrees}");
    }
}

#[test]
fn crossing_behind_takes_the_short_arc_in_both_senses_and_ties_are_deterministic() {
    for (from, to, sense) in [
        (179.0, -179.0, 1.0),
        (-179.0, 179.0, -1.0),
        (170.0, -100.0, 1.0),
    ] {
        let mut follow = settled_at(from * DEG);
        let travel = run_fixed(&mut follow, Some(to * DEG), 30.0);
        let expected = arc(to * DEG, from * DEG);
        assert!(expected * sense > 0.0);
        assert!(close(travel, expected, 1e-9), "{from} -> {to}: {travel}");
        assert!(close(arc(follow.yaw(), to * DEG), 0.0, 1e-12));
    }
    // Empate antipodal exacto: primero el sentido positivo; despues, el del ultimo giro.
    let mut fresh = HorizontalFollow::new();
    fresh.update(Some(PI), DT);
    assert!(fresh.yaw() > 0.0, "{}", fresh.yaw());
    let mut negative = settled_at(0.0);
    negative.update(Some(-PI / 2.0), 1000.0);
    assert_eq!(negative.yaw(), -PI / 2.0);
    negative.update(Some(PI / 2.0), DT);
    assert!(negative.yaw() < -PI / 2.0, "{}", negative.yaw());
    // Muchas vueltas en el mismo sentido: acotado, finito y siguiendo con retraso estable.
    let mut spinning = HorizontalFollow::new();
    let omega = 5.0 * DEG;
    let mut target: f64 = 0.0;
    let mut travel = 0.0;
    for frame in 0..(20.0 * 360.0 / 5.0 / DT) as usize {
        target += omega * DT;
        let before = spinning.yaw();
        spinning.update(Some(wrap_angle(target)), DT);
        travel += arc(spinning.yaw(), before);
        assert!(spinning.yaw() > -PI && spinning.yaw() <= PI);
        if frame as f64 * DT > 20.0 {
            let lag = arc(wrap_angle(target), spinning.yaw());
            let steady = omega * HORIZONTAL_FOLLOW_T95_SECONDS / 20f64.ln();
            assert!(lag > 0.0 && close(lag, steady, 0.2 * DEG), "{frame}: {lag}");
        }
    }
    assert!(travel > 19.0 * 2.0 * PI, "{travel}");
}

#[test]
fn invalid_targets_keep_the_last_goal_and_h_off_returns_to_the_front_without_jumps() {
    // Sin ningun objetivo valido no se mueve.
    let mut idle = HorizontalFollow::new();
    run_fixed(&mut idle, None, 2.0);
    assert_eq!((idle.yaw(), idle.goal()), (0.0, None));
    // Objetivo valido y luego sin direccion: sigue hacia el ultimo valido.
    let mut follow = HorizontalFollow::new();
    follow.update(Some(120.0 * DEG), 1.0);
    let travel = run_fixed(&mut follow, None, 3.0);
    assert!(travel > 0.0 && follow.goal() == Some(120.0 * DEG));
    follow.update(None, 1000.0);
    assert_eq!(follow.yaw(), 120.0 * DEG);
    assert_eq!(follow.error(), Some(0.0));
    // H apagado: el objetivo efectivo es el frente y vuelve con la misma dinamica (sin salto);
    // encendido, retoma el ultimo objetivo. Pausa NO es lo mismo que volver al frente.
    follow.set_active(false);
    assert!(!follow.is_active());
    assert_eq!(follow.effective_goal(), Some(0.0));
    assert_eq!(follow.goal(), Some(120.0 * DEG));
    let back = run_fixed(&mut follow, Some(150.0 * DEG), 1.0);
    assert!(close(back, -HORIZONTAL_FOLLOW_MAX_SPEED, 1e-9), "{back}");
    assert_eq!(follow.goal(), Some(150.0 * DEG));
    follow.set_active(true);
    assert_eq!(follow.effective_goal(), Some(150.0 * DEG));
    let error = follow.error().unwrap();
    assert!(close(error, arc(150.0 * DEG, follow.yaw()), 1e-12));
    // `reset` (presets): frente exacto, sin objetivo previo; conserva H.
    follow.set_active(false);
    follow.reset();
    assert_eq!(
        (follow.yaw(), follow.goal(), follow.speed()),
        (0.0, None, 0.0)
    );
    assert!(!follow.is_active());
}

// --- Fixtures sinteticos (cupula calibrable de las pruebas de la mirada) ---

use umbral::{
    assets::obj::{Model, parse_obj},
    renderer::texture::Texture,
    visual::{
        staged_creature_reveal::EyeRegion,
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

/// Cupula de radio 1 con apice +Z, anillos de 10 grados y 16 segmentos: con `dome_region` las
/// bandas 0-4 son ojo y las 5-7 cuerpo.
fn dome_source() -> String {
    let segments = 16;
    let mut s = String::from("vt 0.5 0.5\nv 0 0 1\nvn 0 0 1\n");
    let mut index = vec![vec![1usize]];
    let mut next = 2;
    for k in 1..=8 {
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
    for k in 1..8 {
        for j in 0..segments {
            let (a, b) = (index[k][j], index[k][(j + 1) % segments]);
            let (c, d) = (index[k + 1][j], index[k + 1][(j + 1) % segments]);
            s += &f(a, c, d);
            s += &f(a, d, b);
        }
    }
    s
}

fn offset_dome_source() -> String {
    dome_source() + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n"
}

/// Cupula con un triangulo de cuerpo desplazado: bounds descentrados como en el asset real.
fn offset_dome() -> Model {
    parse_obj(&offset_dome_source()).unwrap()
}

fn dome_region() -> EyeRegion {
    EyeRegion::new(Vec3::Z, 0.85).unwrap()
}

fn texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160],
    )
    .unwrap()
}

/// Colocacion girada (yaw 0.9) y fuera del origen, distinta de la de demostracion.
fn turned_placement() -> CreaturePlacement {
    CreaturePlacement::new(Vec3::new(40.0, 10.0, -900.0), 80.0, 0.9).unwrap()
}

fn vec_close(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    (a - b).length() <= tolerance
}

// --- H3: transform rigido horizontal alrededor del centro fijo ---

#[test]
fn the_horizontal_turn_is_a_placement_yaw_around_the_fixed_center() {
    let model = offset_dome();
    let bounds_center = Vec3::from_array(model.bounds_center);
    let bounds_radius = model.bounds_radius;
    assert!(
        bounds_center.length() > 0.1,
        "fixture con bounds descentrados"
    );
    let creature = StaticCreature::new(model, texture(), turned_placement()).unwrap();
    let placement = creature.placement();
    let (center, py) = (placement.center(), placement.yaw());
    let base = creature.transform();
    // Reposo: transform y punto exactos.
    assert_eq!(horizontal_transform(&creature, 0.0), base);
    let target = Vec3::new(123.25, -7.5, 310.125);
    assert_eq!(rest_frame_point(center, 0.0, target), target);
    for yaw in [37.0 * DEG, -120.0 * DEG, PI, -179.0 * DEG] {
        let turned = horizontal_transform(&creature, yaw);
        // Lo mismo, bit a bit, que una colocacion con yaw `py + yaw` en el mismo centro: el
        // giro horizontal es solo otra orientacion alrededor de +Y, sin pitch ni alabeo.
        let equivalent = CreaturePlacement::new(center, placement.world_radius(), py + yaw)
            .unwrap()
            .transform_for(bounds_center, bounds_radius)
            .unwrap();
        assert_eq!(turned, equivalent, "{yaw}");
        assert_eq!((turned.rotation.x, turned.rotation.z), (0.0, 0.0));
        let m = turned.matrix();
        assert!(vec_close(m.transform_point(bounds_center), center, 1e-9));
        let g = Mat4::rotation_y(yaw);
        for v in &creature.model().vertices {
            let p = Vec3::from_array(*v);
            let rest = base.matrix().transform_point(p);
            let moved = m.transform_point(p);
            assert!(vec_close(
                moved,
                center + g.transform_dir(rest - center),
                1e-9
            ));
            assert!(close(moved.y, rest.y, 1e-9), "sin giro vertical");
        }
        for n in &creature.model().normals {
            let n = Vec3::from_array(*n);
            assert!(vec_close(
                turned.transform_normal(n),
                g.transform_dir(base.transform_normal(n)),
                1e-12
            ));
        }
        // Inversa: el punto del marco de reposo, girado, vuelve al objetivo.
        let rest = rest_frame_point(center, yaw, target);
        assert!(vec_close(
            center + g.transform_dir(rest - center),
            target,
            1e-9
        ));
        assert!(close(rest.y, target.y, 1e-12));
        assert!(close(
            (rest - center).length(),
            (target - center).length(),
            1e-9
        ));
    }
}

#[test]
fn once_settled_the_real_transformed_front_points_at_the_target_projection() {
    let creature = StaticCreature::new(offset_dome(), texture(), turned_placement()).unwrap();
    let placement = creature.placement();
    let center = placement.center();
    for (azimuth, up) in [(150.0, 300.0), (-100.0, -50.0), (180.0, 0.0), (5.0, 900.0)] {
        let target = point_at(center, placement.yaw(), azimuth * DEG, 500.0, up);
        let mut follow = HorizontalFollow::new();
        follow.update(target_azimuth(center, placement.yaw(), target), 1000.0);
        let front = horizontal_transform(&creature, follow.yaw())
            .matrix()
            .transform_dir(Vec3::Z);
        let to_target = target - center;
        let angle = (front.x * to_target.z - front.z * to_target.x)
            .atan2(front.x * to_target.x + front.z * to_target.z);
        assert!(close(angle, 0.0, 1e-9), "{azimuth}: {angle}");
        assert!(close(front.y, 0.0, 1e-12), "el frente sigue horizontal");
    }
}

// --- H4: ciclo de vida en `World` ---

use umbral::{
    assets::{obj::load_model, ship_model_path},
    scene::world::World,
    visual::{
        creature_eye_comparison::EyeTarget, creature_eye_follow::EyeFollowSettings,
        staged_creature_reveal::StagedRevealSettings,
    },
};

const W: usize = 320;
const H: usize = 180;

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

fn turned_creature() -> StaticCreature {
    StaticCreature::new(offset_dome(), texture(), turned_placement()).unwrap()
}

/// Criatura girada, etapas y mirada a la camara (la comparacion), nave en el arranque cercano.
fn eye_world() -> World {
    let mut world = new_world();
    world.set_creature(turned_creature()).unwrap();
    world.place_near_creature().unwrap();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
    world
        .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
        .unwrap();
    world
}

fn follow_world() -> World {
    let mut world = eye_world();
    world.enable_creature_horizontal_follow().unwrap();
    world
}

#[test]
fn the_follow_needs_the_eye_excludes_the_distant_turn_both_ways_and_dies_with_the_eye() {
    let needs = "el seguimiento horizontal necesita --creature-eye-compare (mirada activa)";
    let mut plain = new_world();
    assert_eq!(plain.enable_creature_horizontal_follow(), Err(needs.into()));
    plain.set_creature(turned_creature()).unwrap();
    plain
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
    assert_eq!(plain.enable_creature_horizontal_follow(), Err(needs.into()));
    assert_eq!(plain.creature_horizontal_follow(), None);
    assert!(plain.set_creature_horizontal_follow_active(false).is_err());

    // Excluyentes en los dos sentidos, sin efectos al fallar.
    let mut distant = eye_world();
    distant.enable_creature_distant_turn().unwrap();
    assert_eq!(
        distant.enable_creature_horizontal_follow(),
        Err("el seguimiento horizontal y el giro global (--creature-distant-turn) son incompatibles".into())
    );
    assert_eq!(distant.creature_horizontal_follow(), None);
    assert!(distant.creature_distant_turn().is_some());
    let mut world = follow_world();
    assert_eq!(
        world.enable_creature_distant_turn(),
        Err("el giro global y el seguimiento horizontal (--creature-horizontal-follow) son incompatibles".into())
    );
    assert_eq!(world.creature_distant_turn(), None);
    assert_eq!(
        world.creature_horizontal_follow(),
        Some(HorizontalFollow::new())
    );

    // H: solo encendido/apagado; disable lo quita; sin seguimiento, `Err` sin efectos.
    world.set_creature_horizontal_follow_active(false).unwrap();
    assert!(!world.creature_horizontal_follow().unwrap().is_active());
    world.set_creature_horizontal_follow_active(true).unwrap();
    assert!(world.creature_horizontal_follow().unwrap().is_active());
    world.disable_creature_horizontal_follow();
    assert_eq!(world.creature_horizontal_follow(), None);
    assert!(world.set_creature_horizontal_follow_active(true).is_err());
    assert!(world.creature_eye_follow().is_some(), "la mirada sigue");
    world.enable_creature_distant_turn().unwrap();

    // Apagar la mirada o reiniciar/cambiar el modo lo quita; reactivarla no lo trae.
    for reset in 0..3 {
        let mut world = follow_world();
        match reset {
            0 => world.disable_creature_eye_follow(),
            1 => world
                .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
                .unwrap(),
            _ => world.disable_staged_creature_reveal(),
        }
        assert_eq!(world.creature_horizontal_follow(), None, "{reset}");
        if reset < 2 {
            world
                .enable_creature_eye_follow(EyeFollowSettings::default())
                .unwrap();
            assert_eq!(world.creature_horizontal_follow(), None, "{reset}");
        }
    }
    // El transform de dibujo (QA): sin criatura `None`; en reposo, el original exacto.
    let mut world = follow_world();
    assert_eq!(
        world.creature_render_transform(),
        Some(world.creature().unwrap().transform())
    );
    world.clear_creature();
    assert_eq!(world.creature_render_transform(), None);
    assert!(
        world.creature_horizontal_follow().is_some(),
        "el estado se conserva"
    );
}

// --- H5: el cuerpo sigue cerca y lejos con segundos reales y el ojo compensa ---

use umbral::{
    scene::ship::ShipInput,
    visual::{creature_distant_turn::CreaturePreset, creature_eye_follow::GazeAngles},
};

/// Salto de posicion de la CAMARA (prueba unitaria, no fisica): en el plano del centro, a
/// `radii` radios y `azimuth` del frente de reposo, mas `up`.
fn put_camera(world: &mut World, azimuth: f64, radii: f64, up: f64) {
    let p = world.creature().unwrap().placement();
    world.camera.position = point_at(p.center(), p.yaw(), azimuth, radii * p.world_radius(), up);
}

fn follow_yaw(world: &World) -> f64 {
    world.creature_horizontal_follow().unwrap().yaw()
}

#[test]
fn near_and_far_the_body_follows_the_chosen_point_on_every_side_with_real_seconds() {
    for (radii, azimuth) in [
        (2.6, 90.0),
        (2.6, 180.0),
        (8.3, -120.0),
        (8.3, 179.0),
        (1.5, -60.0),
    ] {
        let mut world = follow_world();
        put_camera(&mut world, azimuth * DEG, radii, 40.0);
        // `update` (fisica, dt limitado) no lo avanza; F5 tampoco lo congela.
        world.toggles.freeze_orbits = true;
        let camera = world.camera.position;
        world.update(&ShipInput::default(), 0.1);
        world.camera.position = camera;
        assert_eq!(follow_yaw(&world), 0.0);
        world.update_creature_eye_follow(30.0);
        assert!(
            close(arc(follow_yaw(&world), azimuth * DEG), 0.0, 1e-9),
            "{radii} {azimuth}"
        );
    }
    // G elige el punto: posicion de la nave o de la camara, nunca `camera.target`.
    let mut world = follow_world();
    let p = turned_placement();
    world.ship.position = point_at(p.center(), p.yaw(), -70.0 * DEG, 300.0, 0.0);
    put_camera(&mut world, 100.0 * DEG, 3.0, 0.0);
    world.camera.target = point_at(p.center(), p.yaw(), 10.0 * DEG, 300.0, 0.0);
    world.update_creature_eye_follow(60.0);
    assert!(close(arc(follow_yaw(&world), 100.0 * DEG), 0.0, 1e-9));
    world.set_creature_eye_target(EyeTarget::Ship).unwrap();
    world.update_creature_eye_follow(60.0);
    assert!(close(arc(follow_yaw(&world), -70.0 * DEG), 0.0, 1e-9));
    // Oculta por O2 tambien avanza (solo el estado; la pose es perezosa al dibujar).
    let mut hidden = follow_world();
    hidden.debug_oxygen01 = 1.0;
    hidden.snap_creature_reveal();
    put_camera(&mut hidden, 45.0 * DEG, 3.0, 0.0);
    hidden.update_creature_eye_follow(1.0);
    assert!(follow_yaw(&hidden) > 0.0);
    // Sin direccion XZ (camara justo encima del centro): conserva el ultimo objetivo.
    let mut above = follow_world();
    put_camera(&mut above, 60.0 * DEG, 3.0, 0.0);
    above.update_creature_eye_follow(1.0);
    let goal = above.creature_horizontal_follow().unwrap().goal();
    above.camera.position = p.center() + Vec3::Y * 500.0;
    above.update_creature_eye_follow(60.0);
    assert_eq!(above.creature_horizontal_follow().unwrap().goal(), goal);
    assert!(close(follow_yaw(&above), goal.unwrap(), 1e-12));
}

/// Mundo de referencia SIN seguimiento cuya criatura ya esta colocada con yaw `py + yaw`.
fn rotated_reference(yaw: f64, camera: Vec3) -> World {
    let p = turned_placement();
    let placement = CreaturePlacement::new(p.center(), p.world_radius(), p.yaw() + yaw).unwrap();
    let mut world = new_world();
    world
        .set_creature(StaticCreature::new(offset_dome(), texture(), placement).unwrap())
        .unwrap();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
    world
        .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
        .unwrap();
    world.camera.position = camera;
    world.update_creature_eye_follow(100.0);
    world
}

fn gaze_close(a: GazeAngles, b: GazeAngles, tolerance: f64) -> bool {
    close(a.yaw(), b.yaw(), tolerance) && close(a.pitch(), b.pitch(), tolerance)
}

#[test]
fn the_eye_compensates_the_real_body_turn_like_a_creature_placed_already_turned() {
    // Cuerpo a mitad de camino (no asentado) y camara arriba: el ojo debe pedir lo mismo que
    // una criatura colocada ya con ese giro (otra ruta de codigo: `transform_for` + `aim`).
    for (azimuth, up, seconds) in [(130.0, 60.0, 3.0), (-160.0, -40.0, 5.0), (40.0, 120.0, 0.5)] {
        let mut world = follow_world();
        put_camera(&mut world, azimuth * DEG, 4.0, up);
        world.update_creature_eye_follow(seconds);
        let yaw = follow_yaw(&world);
        assert!(yaw != 0.0);
        let reference = rotated_reference(yaw, world.camera.position);
        assert_eq!(
            world.creature_render_transform(),
            reference.creature_render_transform(),
            "{azimuth}: mismo transform para ojo y cuerpo"
        );
        let (goal, expected) = (
            world.creature_eye_goal().unwrap(),
            reference.creature_eye_goal().unwrap(),
        );
        assert!(
            gaze_close(goal, expected, 1e-9),
            "{azimuth}: {goal:?} {expected:?}"
        );
        // Y el ojo de la candidata persigue ese objetivo (despues del cuerpo en el mismo paso).
        world.update_creature_eye_follow(100.0);
        let settled = rotated_reference(follow_yaw(&world), world.camera.position);
        assert!(gaze_close(
            world.creature_eye_follow().unwrap().current(),
            settled.creature_eye_follow().unwrap().current(),
            1e-9
        ));
    }
}

#[test]
fn presets_reset_the_body_and_creature_changes_keep_it() {
    let mut world = follow_world();
    world.debug_oxygen01 = 0.25;
    put_camera(&mut world, 150.0 * DEG, 3.0, 0.0);
    world.update_creature_eye_follow(4.0);
    assert!(follow_yaw(&world) > 1.0);
    world.set_creature_horizontal_follow_active(false).unwrap();
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    let follow = world.creature_horizontal_follow().unwrap();
    assert_eq!(
        (follow.yaw(), follow.goal(), follow.is_active()),
        (0.0, None, false)
    );
    assert_eq!(world.debug_oxygen01, 0.25);
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    let creature = world.creature().unwrap();
    let skin = world.creature_eye_skin().unwrap();
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        skin.aim(creature, world.camera.position)
    );
    // clear/set conservan el giro; el ojo salta al punto compensado.
    world.set_creature_horizontal_follow_active(true).unwrap();
    world.update_creature_eye_follow(3.0);
    let kept = world.creature_horizontal_follow().unwrap();
    assert!(kept.yaw() != 0.0);
    world.clear_creature();
    world.update_creature_eye_follow(3.0);
    assert_eq!(
        world.creature_horizontal_follow(),
        Some(kept),
        "sin criatura no avanza"
    );
    world.set_creature(turned_creature()).unwrap();
    assert_eq!(world.creature_horizontal_follow(), Some(kept));
    let reference = rotated_reference(kept.yaw(), world.camera.position);
    assert!(gaze_close(
        world.creature_eye_follow().unwrap().current(),
        reference.creature_eye_goal().unwrap(),
        1e-9
    ));
}

// --- H6: rutas de render (caracterizacion; ver mutaciones en la evidencia) ---

use umbral::{
    renderer::{framebuffer::Framebuffer, stats::RenderStats},
    scene::world::SPACE_COLOR,
};

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

fn with_oxygen(mut world: World, oxygen: f64) -> World {
    world.debug_oxygen01 = oxygen;
    world.snap_creature_reveal();
    world
}

#[test]
fn a_body_at_rest_draws_exactly_the_old_paths() {
    for (oxygen, preset) in [
        (0.05, CreaturePreset::Near),
        (0.25, CreaturePreset::Near),
        (0.05, CreaturePreset::Far),
        (1.0, CreaturePreset::Far),
    ] {
        let mut follow = with_oxygen(follow_world(), oxygen);
        let mut plain = with_oxygen(eye_world(), oxygen);
        follow.place_creature_preset(preset).unwrap();
        plain.place_creature_preset(preset).unwrap();
        assert_eq!(follow_yaw(&follow), 0.0);
        let (a, b) = (render(&follow), render(&plain));
        assert!(a.0 == b.0, "{oxygen} {preset:?}");
        assert_eq!(a.1, b.1);
        assert_eq!(follow.creature_eye_posed(), plain.creature_eye_posed());
    }
}

#[test]
fn a_turned_body_draws_both_parts_turned_even_with_the_eye_at_rest() {
    for oxygen in [0.05, 0.25] {
        let mut world = with_oxygen(follow_world(), oxygen);
        put_camera(&mut world, 120.0 * DEG, 3.0, 0.0);
        world.update_creature_eye_follow(3.0);
        let mut still = with_oxygen(follow_world(), oxygen);
        put_camera(&mut still, 120.0 * DEG, 3.0, 0.0);
        still.update_creature_eye_follow(3.0);
        still.disable_creature_horizontal_follow();
        let ((turned, ts), (rest, rs)) = (render(&world), render(&still));
        assert!(turned != rest, "{oxygen}");
        assert_eq!(
            (ts.objects_submitted, ts.triangles_submitted),
            (rs.objects_submitted, rs.triangles_submitted)
        );
    }
    // Ojo exactamente en reposo con el cuerpo aun girado: el cuerpo se asienta mirando a la nave
    // detras de la criatura, `H` lo apaga y, mientras vuelve al frente (13 s desde 180 grados:
    // le queda ~0.9 grados), la nave queda detras del ojo.
    let setup = || {
        let mut world = with_oxygen(follow_world(), 0.05);
        world.set_creature_eye_target(EyeTarget::Ship).unwrap();
        let p = turned_placement();
        world.ship.position = point_at(p.center(), p.yaw(), 180.0 * DEG, 300.0, 0.0);
        world.update_creature_eye_follow(60.0);
        world.set_creature_horizontal_follow_active(false).unwrap();
        world.update_creature_eye_follow(13.0);
        world
    };
    let world = setup();
    assert!(world.creature_eye_follow().unwrap().current().is_neutral());
    assert!(
        follow_yaw(&world).abs() > 0.5 * DEG,
        "{}",
        follow_yaw(&world)
    );
    let mut still = setup();
    still.disable_creature_horizontal_follow();
    assert!(render(&world).0 != render(&still).0);
}

#[test]
fn culled_or_hidden_creatures_never_pose_and_count_like_the_static_one() {
    let mut world = with_oxygen(follow_world(), 0.05);
    put_camera(&mut world, 120.0 * DEG, 3.0, 0.0);
    world.update_creature_eye_follow(3.0);
    let center = turned_placement().center();
    world.camera.target = world.camera.position + (world.camera.position - center);
    let poses = world.creature_eye_pose_updates();
    let (_, stats) = render(&world);
    let faces = world.creature().unwrap().model().faces.len();
    let mut empty = with_oxygen(follow_world(), 0.05);
    empty.camera = world.camera;
    empty.clear_creature();
    let ship = render(&empty).1;
    assert_eq!(stats.objects_culled, ship.objects_culled + 1);
    assert_eq!(stats.triangles_culled, ship.triangles_culled + faces);
    assert_eq!(stats.vertices_processed, ship.vertices_processed);
    assert_eq!(world.creature_eye_pose_updates(), poses);
    let mut hidden = with_oxygen(follow_world(), 1.0);
    put_camera(&mut hidden, 120.0 * DEG, 3.0, 0.0);
    hidden.update_creature_eye_follow(3.0);
    let poses = hidden.creature_eye_pose_updates();
    let (pixels, stats) = render(&hidden);
    let mut nothing = with_oxygen(follow_world(), 1.0);
    nothing.camera = hidden.camera;
    nothing.clear_creature();
    assert!(pixels == render(&nothing).0);
    assert_eq!(stats, render(&nothing).1);
    assert_eq!(hidden.creature_eye_pose_updates(), poses);
}

// --- H7: panel del seguimiento (regla y rapidez reales, no los 20/12/fade de la 01) ---

use umbral::visual::{
    creature_distant_turn::{distant_turn_panel, draw_distant_turn_panel},
    creature_eye_comparison::eye_comparison_panel,
};

fn ascii_for_the_font(line: &str) -> bool {
    line.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || " -.:/%()=+,|".contains(c))
}

#[test]
fn the_panel_text_shows_h_presets_body_goal_error_speed_and_the_real_rule() {
    let mut follow = HorizontalFollow::new();
    follow.update(Some(130.0 * DEG), 2.0);
    let status = HorizontalFollowStatus {
        follow,
        target_azimuth: Some(130.0 * DEG),
        has_creature: true,
    };
    let yaw = follow.yaw().to_degrees();
    let error = follow.error().unwrap().to_degrees();
    assert_eq!(
        horizontal_follow_lines(&status, CreaturePreset::Far),
        [
            "SEGUIMIENTO HORIZONTAL: ON   H APAGA".to_string(),
            "7 CERCA  8 LEJOS  R REPITE LEJOS".to_string(),
            format!("CUERPO YAW {yaw:+.1}  OBJETIVO +130.0"),
            format!("ERROR {error:+.1}  RAPIDEZ 18.0 GRADOS/S"),
            "REGLA: T95 4 S  TOPE 18 GRADOS/S  SOLO YAW".to_string(),
        ]
    );
    // Apagado: lo dice (vuelve al frente; no es pausa) y el objetivo efectivo es el frente.
    let mut off = follow;
    off.set_active(false);
    let lines = horizontal_follow_lines(
        &HorizontalFollowStatus {
            follow: off,
            ..status
        },
        CreaturePreset::Near,
    );
    assert_eq!(lines[0], "SEGUIMIENTO: OFF  H ENCIENDE (AL FRENTE)");
    assert_eq!(lines[1], "7 CERCA  8 LEJOS  R REPITE CERCA");
    assert!(lines[2].ends_with("OBJETIVO +0.0"), "{}", lines[2]);
    // Sin direccion XZ: mantiene el ultimo objetivo y lo avisa.
    let lost = HorizontalFollowStatus {
        target_azimuth: None,
        ..status
    };
    let lines = horizontal_follow_lines(&lost, CreaturePreset::Far);
    assert_eq!(lines[4], "SIN DIRECCION XZ: MANTIENE EL ULTIMO");
    assert_eq!(lines.len(), 6);
    // Sin objetivo todavia y sin criatura.
    let fresh = HorizontalFollowStatus {
        follow: HorizontalFollow::new(),
        target_azimuth: None,
        has_creature: true,
    };
    let lines = horizontal_follow_lines(&fresh, CreaturePreset::Far);
    assert_eq!(lines[2], "CUERPO YAW +0.0  OBJETIVO --");
    assert_eq!(lines[3], "ERROR --  RAPIDEZ 0.0 GRADOS/S");
    let none = HorizontalFollowStatus {
        has_creature: false,
        ..fresh
    };
    assert_eq!(
        horizontal_follow_lines(&none, CreaturePreset::Far)[2..],
        ["SIN CRIATURA: SIN SEGUIMIENTO".to_string()]
    );
    // Texto de la fuente 5x7 y lineas cortas; cabe junto al panel del ojo (layout de la 01).
    for s in [status, lost, fresh, none] {
        let lines = horizontal_follow_lines(&s, CreaturePreset::Far);
        for line in &lines {
            assert!(ascii_for_the_font(line) && line.len() <= 44, "{line}");
        }
        for (w, h) in [(640, 360), (1280, 720)] {
            let left = eye_comparison_panel(
                w,
                h,
                &["LIMITE 10/8: DESVANECE   EJE 104.9 GRADOS (FADE 75-105)".to_string()],
            );
            let panel = distant_turn_panel(w, h, &lines, &left);
            assert_eq!((panel.x + panel.w, panel.y + panel.h), (w, h));
            let mut fb = Framebuffer::new(w, h, 0xFFFFFF).unwrap();
            draw_distant_turn_panel(&mut fb, &lines, &left);
            assert_eq!(fb.pixels[(h - 1) * w + w - 1], 0x727272);
        }
    }
}

#[test]
fn the_world_reports_the_follow_status_for_the_chosen_point() {
    let mut world = follow_world();
    put_camera(&mut world, -135.0 * DEG, 3.0, 0.0);
    world.update_creature_eye_follow(1.0);
    let status = world.creature_horizontal_follow_status().unwrap();
    assert_eq!(status.follow, world.creature_horizontal_follow().unwrap());
    assert!(status.has_creature);
    assert!(close(
        arc(status.target_azimuth.unwrap(), -135.0 * DEG),
        0.0,
        1e-9
    ));
    world.camera.position = turned_placement().center() - Vec3::Y * 50.0;
    let status = world.creature_horizontal_follow_status().unwrap();
    assert_eq!(status.target_azimuth, None);
    world.clear_creature();
    let status = world.creature_horizontal_follow_status().unwrap();
    assert!(!status.has_creature && status.target_azimuth.is_none());
    assert_eq!(eye_world().creature_horizontal_follow_status(), None);
}

// --- H8: linea de comandos, entrada de biblioteca y arranque ---

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::{
    app::{GameOptions, run_game_with_horizontal_follow},
    visual::{
        creature_eye_comparison::{draw_eye_comparison_panel, eye_comparison_lines},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::CreatureOptions,
    },
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "umbral-horizontal-follow-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_texture(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160])
        .unwrap();
    writer.finish().unwrap();
}

/// `offset_dome` escalada y movida al centro de la ROI calibrada del asset real (la que usa la
/// linea de comandos).
fn calibrated_source() -> String {
    let (c, s) = (
        CREATURE_EYE_REGION.center(),
        CREATURE_EYE_REGION.radius() / 0.85,
    );
    let mut out = String::new();
    for line in offset_dome_source().lines() {
        match line.strip_prefix("v ") {
            Some(rest) => {
                let p: Vec<f64> = rest.split(' ').map(|x| x.parse().unwrap()).collect();
                let q = c + (Vec3::new(p[0], p[1], p[2]) - Vec3::Z) * s;
                out += &format!("v {} {} {}\n", q.x, q.y, q.z);
            }
            None => out += &format!("{line}\n"),
        }
    }
    out
}

/// Siempre con `--headless`: estas pruebas nunca abren ventana.
fn run_game(args: &[OsString]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .output()
        .unwrap()
}

fn decode(bytes: &[u8]) -> (usize, usize, Vec<u32>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let pixels = data[..info.buffer_size()]
        .chunks_exact(3)
        .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
        .collect();
    (info.width as usize, info.height as usize, pixels)
}

fn args(parts: &[&OsStr]) -> Vec<OsString> {
    parts.iter().map(|p| p.to_os_string()).collect()
}

#[test]
fn the_command_line_rejects_bad_combinations_before_any_io() {
    let dir = temp_dir("cli-guards");
    let (model, texture_path) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    let (no_model, no_texture) = (dir.join("no.obj"), dir.join("no.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    write_texture(&texture_path);
    let texture_bytes = std::fs::read(&texture_path).unwrap();
    let output = dir.join("salida.png");
    let o = OsStr::new;
    let missing = |extra: &[&str]| {
        let mut a = args(&[
            o("--creature-model"),
            no_model.as_os_str(),
            o("--creature-texture"),
            no_texture.as_os_str(),
            o("--output"),
            output.as_os_str(),
        ]);
        a.extend(extra.iter().map(OsString::from));
        a
    };
    let full = [
        "--creature-reveal-stages",
        "--creature-eye-follow",
        "--creature-eye-compare",
    ];
    let cases = [
        (
            missing(&[
                "--creature-reveal-stages",
                "--creature-eye-follow",
                "--creature-horizontal-follow",
            ]),
            "--creature-horizontal-follow necesita --creature-eye-compare",
        ),
        (
            missing(
                &[
                    full.as_slice(),
                    &["--creature-horizontal-follow", "--creature-distant-turn"],
                ]
                .concat(),
            ),
            "--creature-horizontal-follow y --creature-distant-turn son incompatibles: elige uno",
        ),
        (
            missing(
                &[
                    full.as_slice(),
                    &["--creature-reveal", "--creature-horizontal-follow"],
                ]
                .concat(),
            ),
            "incompatibles",
        ),
        (
            missing(&[
                "--creature-reveal-stages",
                "--creature-eye-compare",
                "--creature-horizontal-follow",
            ]),
            "--creature-eye-compare necesita --creature-eye-follow",
        ),
        (
            args(&[
                o("--legacy"),
                o("--creature-horizontal-follow"),
                o("--output"),
                output.as_os_str(),
            ]),
            "--legacy no admite --creature-horizontal-follow",
        ),
    ];
    for (arguments, needle) in cases {
        let out = run_game(&arguments);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{needle}: {stderr}");
        assert!(stderr.contains(needle), "{needle:?} no esta en {stderr:?}");
        assert!(!output.exists() && !no_model.exists() && !no_texture.exists());
    }
    // Alias de la salida con la textura: rechazado sin tocar la entrada.
    let mut alias = args(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture_path.as_os_str(),
    ]);
    alias.extend(full.iter().map(OsString::from));
    alias.extend(args(&[
        o("--creature-horizontal-follow"),
        o("--output"),
        texture_path.as_os_str(),
    ]));
    let out = run_game(&alias);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("coincide con la entrada"));
    assert_eq!(std::fs::read(&texture_path).unwrap(), texture_bytes);
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.contains("--creature-horizontal-follow (requiere --creature-eye-compare; no con --creature-distant-turn)"),
        "{help}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_library_entry_point_validates_before_any_io() {
    let dir = temp_dir("library");
    let (model, texture_path, output) = (
        dir.join("no.obj"),
        dir.join("no.png"),
        dir.join("salida.png"),
    );
    let requested = || {
        CreatureOptions::from_flags(
            Some(model.clone()),
            Some(texture_path.clone()),
            false,
            None,
            false,
        )
        .unwrap()
    };
    let options = |creature: bool, v1: bool, stages: bool, eye: bool, out: &Path| GameOptions {
        width: 64,
        height: 36,
        headless: true,
        frame_limit: None,
        output: out.to_path_buf(),
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: if creature { requested() } else { None },
        creature_reveal: v1,
        creature_reveal_stages: stages,
        creature_eye_follow: eye,
        debug_oxygen: 0.05,
    };
    for (o, needle) in [
        (
            options(true, false, true, false, &output),
            "--creature-eye-compare necesita --creature-eye-follow",
        ),
        (
            options(true, false, false, true, &output),
            "--creature-eye-follow necesita --creature-reveal-stages",
        ),
        (
            options(false, false, true, true, &output),
            "--creature-eye-follow necesita --creature-model",
        ),
        (options(true, true, true, true, &output), "incompatibles"),
    ] {
        let error = run_game_with_horizontal_follow(o).expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
        assert!(!output.exists() && !model.exists() && !texture_path.exists());
    }
    let (real_model, real_texture) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&real_model, calibrated_source()).unwrap();
    write_texture(&real_texture);
    let before = std::fs::read(&real_texture).unwrap();
    let alias = GameOptions {
        output: real_texture.clone(),
        creature: CreatureOptions::from_flags(
            Some(real_model.clone()),
            Some(real_texture.clone()),
            false,
            None,
            false,
        )
        .unwrap(),
        ..options(false, false, true, true, &output)
    };
    let error = run_game_with_horizontal_follow(alias).expect_err("alias");
    assert!(error.contains("coincide con la entrada"), "{error}");
    assert_eq!(std::fs::read(&real_texture).unwrap(), before);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_game_starts_far_and_logs_the_gaze_after_the_preset_with_both_panels() {
    let dir = temp_dir("cli-start");
    let (model, texture_path) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    write_texture(&texture_path);
    let output = dir.join("lejos.png");
    let o = OsStr::new;
    let arguments = args(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture_path.as_os_str(),
        o("--near-creature"),
        o("--creature-reveal-stages"),
        o("--creature-eye-follow"),
        o("--creature-eye-compare"),
        o("--creature-horizontal-follow"),
        o("--debug-oxygen"),
        o("0.05"),
        o("--width"),
        o("640"),
        o("--height"),
        o("360"),
        o("--output"),
        output.as_os_str(),
    ]);
    let out = run_game(&arguments);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // El mismo arranque armado con la API publica: LEJOS (aunque se pida --near-creature),
    // cuerpo de frente, ojo a la camara y los dos paneles.
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 640.0 / 360.0).unwrap();
    world.debug_oxygen01 = 0.05;
    let creature = StaticCreature::load(&model, &texture_path, CreaturePlacement::demo()).unwrap();
    world.set_creature(creature).unwrap();
    world.place_near_creature().unwrap();
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), CREATURE_EYE_REGION)
        .unwrap();
    world
        .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
        .unwrap();
    world.enable_creature_horizontal_follow().unwrap();
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    let gaze = world.creature_eye_follow().unwrap().current();
    let status = world.creature_horizontal_follow_status().unwrap();
    // Una sola linea de mirada, la del preset efectivo, despues del aviso del seguimiento.
    let gaze_line = format!(
        "Mirada (tras el preset LEJOS): yaw {:+.3} grados, pitch {:+.3} grados hacia la camara | t95 1.5 s (candidato)",
        gaze.yaw().to_degrees(),
        gaze.pitch().to_degrees()
    );
    let follow_line = format!(
        "Seguimiento horizontal (candidata 02): arranque LEJOS 8 radios a 25 grados | cuerpo yaw +0.0, objetivo {:+.1} grados (camara) | t95 4 s, tope 18 grados/s, solo yaw, cerca y lejos | H/7/8/R",
        status.target_azimuth.unwrap().to_degrees()
    );
    assert_eq!(stdout.matches("Mirada").count(), 1, "{stdout}");
    let (f, g) = (stdout.find(&follow_line), stdout.find(&gaze_line));
    assert!(
        f.is_some() && g.is_some() && f < g,
        "{stdout}\n{follow_line}\n{gaze_line}"
    );
    assert!(stdout.contains("Comparacion de la mirada: G alterna NAVE/CAMARA"));
    let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
    world.render(&mut fb);
    let lines = eye_comparison_lines(&world.creature_eye_comparison_status().unwrap());
    draw_eye_comparison_panel(&mut fb, &lines);
    let left = eye_comparison_panel(640, 360, &lines);
    let follow_lines = horizontal_follow_lines(&status, CreaturePreset::Far);
    assert_eq!(follow_lines[0], "SEGUIMIENTO HORIZONTAL: ON   H APAGA");
    draw_distant_turn_panel(&mut fb, &follow_lines, &left);
    let (w, h, pixels) = decode(&std::fs::read(&output).unwrap());
    assert_eq!((w, h), (640, 360));
    assert!(
        pixels == fb.pixels,
        "el PNG headless es el cuadro de la candidata"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- H9: vuelo continuo real (`ShipInput -> World::update -> camara -> cuerpo -> ojo`) ---

/// Una muestra por cuadro del vuelo.
#[derive(Clone, Copy, Debug)]
struct Sample {
    target: Option<f64>,
    body: f64,
    speed: f64,
    radii: f64,
}

/// Rodea la criatura en sentido antihorario visto desde +Y a ~`orbit_radii` radios (nave), a
/// crucero (60 u/s) con un controlador de rumbo: la nave pasa por los costados, por detras y por
/// el corte de +-180 grados varias veces. Fisica real a 60 Hz; renderiza cada 30 cuadros.
fn orbit(world: &mut World, orbit_radii: f64, seconds: f64) -> Vec<Sample> {
    let p = world.creature().unwrap().placement();
    let (center, r0) = (p.center(), orbit_radii * p.world_radius());
    let mut samples = Vec::new();
    for frame in 0..(seconds / DT) as usize {
        let offset = world.ship.position - center;
        let radial = Vec3::new(offset.x, 0.0, offset.z).normalized();
        let tangent = Vec3::Y.cross(radial);
        let r = Vec3::new(offset.x, 0.0, offset.z).length();
        let correction = ((r - r0) / (0.3 * r0)).clamp(-1.0, 1.0);
        let desired = (tangent - radial * correction).normalized();
        let forward = world.ship.forward();
        let turn = forward.z * desired.x - forward.x * desired.z;
        let input = ShipInput {
            thrust: 1.0,
            yaw: (turn * 4.0).clamp(-1.0, 1.0),
            ..ShipInput::default()
        };
        world.update(&input, DT);
        world.update_creature_eye_follow(DT);
        if frame % 30 == 0 {
            let (pixels, stats) = render(world);
            assert!(stats.objects_submitted >= 2);
            assert!(pixels.iter().any(|&q| q != SPACE_COLOR));
        }
        let status = world.creature_horizontal_follow_status().unwrap();
        samples.push(Sample {
            target: status.target_azimuth,
            body: status.follow.yaw(),
            speed: status.follow.speed(),
            radii: (world.camera.position - center).length() / p.world_radius(),
        });
    }
    samples
}

/// Comprueba el vuelo: rapidez acotada, objetivo que da vueltas completas, cuerpo que lo sigue
/// con retraso acotado tras el arranque y que nunca vuelve hacia el frente (gira siempre en el
/// sentido del vuelo). Devuelve (vueltas del objetivo, error maximo asentado en grados).
fn check_orbit(samples: &[Sample], settle_s: f64, max_error_deg: f64) -> (f64, f64) {
    let mut target_travel = 0.0;
    let mut worst: f64 = 0.0;
    for (i, pair) in samples.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        assert!(
            b.speed <= HORIZONTAL_FOLLOW_MAX_SPEED + 1e-12,
            "{i}: {}",
            b.speed
        );
        assert!(b.body.is_finite() && b.body > -PI && b.body <= PI);
        target_travel += arc(b.target.unwrap(), a.target.unwrap());
        if i as f64 * DT > settle_s {
            let step = arc(b.body, a.body);
            assert!(step >= 0.0, "{i}: el cuerpo retrocede {step}");
            let error = arc(b.target.unwrap(), b.body).abs().to_degrees();
            worst = worst.max(error);
        }
    }
    assert!(worst < max_error_deg, "error asentado {worst}");
    (target_travel / (2.0 * PI), worst)
}

#[test]
fn flying_around_near_and_far_the_body_follows_through_the_back_and_the_wrap() {
    for (preset, orbit_radii, max_error, min_radii, max_radii) in [
        (CreaturePreset::Near, 2.8, 30.0, 2.5, 3.6),
        (CreaturePreset::Far, 8.0, 12.0, 7.0, 9.0),
    ] {
        let mut world = with_oxygen(follow_world(), 0.05);
        world.place_creature_preset(preset).unwrap();
        let seconds = if preset == CreaturePreset::Near {
            70.0
        } else {
            160.0
        };
        let samples = orbit(&mut world, orbit_radii, seconds);
        let (laps, worst) = check_orbit(&samples, 20.0, max_error);
        assert!(laps > 1.1, "{preset:?}: {laps} vueltas");
        let settled = &samples[(20.0 / DT) as usize..];
        assert!(
            settled
                .iter()
                .all(|s| (min_radii..max_radii).contains(&s.radii)),
            "{preset:?}"
        );
        println!("ORBITA {preset:?}: {laps:.2} vueltas, error asentado maximo {worst:.2} grados");
        // El ojo sigue compensando contra el giro real al final del vuelo.
        let reference = rotated_reference(follow_yaw(&world), world.camera.position);
        assert!(gaze_close(
            world.creature_eye_goal().unwrap(),
            reference.creature_eye_goal().unwrap(),
            1e-9
        ));
        assert_eq!(
            world.creature_render_transform(),
            reference.creature_render_transform()
        );
    }
}

/// Asset real de Charlie, solo lectura, con rutas por entorno (nunca en el codigo):
/// `UMBRAL_REAL_CREATURE_OBJ` y `UMBRAL_REAL_CREATURE_TEXTURE`. Se ejecuta a mano con
/// `--ignored`; sin las variables falla, no pasa en vacio.
#[test]
#[ignore = "asset real privado: rutas por variables de entorno"]
fn the_real_asset_follows_around_near_and_far_and_keeps_its_arrays() {
    let obj = PathBuf::from(std::env::var_os("UMBRAL_REAL_CREATURE_OBJ").expect("ruta del OBJ"));
    let png = PathBuf::from(
        std::env::var_os("UMBRAL_REAL_CREATURE_TEXTURE").expect("ruta de la textura"),
    );
    for (preset, orbit_radii, max_error) in [
        (CreaturePreset::Near, 2.8, 30.0),
        (CreaturePreset::Far, 8.0, 12.0),
    ] {
        let mut world = World::new(load_model(&ship_model_path()).unwrap(), 640.0 / 360.0).unwrap();
        world.debug_oxygen01 = 0.05;
        world
            .set_creature(StaticCreature::load(&obj, &png, CreaturePlacement::demo()).unwrap())
            .unwrap();
        world
            .enable_staged_creature_reveal(StagedRevealSettings::default(), CREATURE_EYE_REGION)
            .unwrap();
        world
            .enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)
            .unwrap();
        world.enable_creature_horizontal_follow().unwrap();
        world.place_creature_preset(preset).unwrap();
        let seconds = if preset == CreaturePreset::Near {
            70.0
        } else {
            160.0
        };
        let samples = orbit(&mut world, orbit_radii, seconds);
        let (laps, worst) = check_orbit(&samples, 20.0, max_error);
        println!(
            "REAL_ASSET {preset:?}: {laps:.2} vueltas, error asentado maximo {worst:.2} grados"
        );
        // Transform rigido: el de una colocacion con el yaw total, centro fijo.
        let creature = world.creature().unwrap();
        let p = creature.placement();
        let expected =
            CreaturePlacement::new(p.center(), p.world_radius(), p.yaw() + follow_yaw(&world))
                .unwrap()
                .transform_for(
                    Vec3::from_array(creature.model().bounds_center),
                    creature.model().bounds_radius,
                )
                .unwrap();
        assert_eq!(world.creature_render_transform(), Some(expected));
        let fresh = load_model(&obj).unwrap();
        let model = creature.model();
        assert!(model.vertices == fresh.vertices && model.uvs == fresh.uvs);
        assert!(model.normals == fresh.normals && model.bounds_center == fresh.bounds_center);
        assert!(model.faces.iter().zip(&fresh.faces).all(|(a, b)| {
            a.vertex_indices == b.vertex_indices
                && a.uv_indices == b.uv_indices
                && a.normal_indices == b.normal_indices
        }));
    }
}

#[test]
fn tiny_elapsed_cannot_snap_faster_than_the_actual_motion_limit() {
    for dt in [1e-12, 1e-9, f64::MIN_POSITIVE, f64::from_bits(1)] {
        for goal in [1e-7, -1e-7, 1e-16, -1e-16] {
            let mut follow = HorizontalFollow::new();
            follow.update(Some(goal), dt);
            let distance = wrap_angle(follow.yaw()).abs();
            assert!(follow.yaw().is_finite() && follow.speed().is_finite());
            assert!(
                distance / dt <= HORIZONTAL_FOLLOW_MAX_SPEED * (1.0 + 1e-12),
                "goal={goal:e} dt={dt:e} yaw={} speed={}",
                follow.yaw(),
                follow.speed()
            );
            assert!(follow.speed() <= HORIZONTAL_FOLLOW_MAX_SPEED * (1.0 + 1e-12));
            assert!(distance <= goal.abs());
            assert_eq!(follow.goal(), Some(goal));
            follow.update(None, 1.0);
            assert_eq!(
                follow.yaw(),
                goal,
                "the retained tiny goal must still settle"
            );
        }
    }
}

#[test]
fn tiny_elapsed_from_nonzero_yaw_preserves_the_geometric_rate_and_valid_goals() {
    for start in [0.7, -0.7, PI - 1e-10, -PI + 1e-10] {
        for dt in [1e-9, 1e-12, f64::MIN_POSITIVE, f64::from_bits(1)] {
            for offset in [1.0, -1.0, 1e-7, -1e-7, f64::EPSILON] {
                let mut follow = settled_at(start);
                let goal = wrap_angle(start + offset);
                follow.update(Some(goal), dt);
                let step = wrap_angle(follow.yaw() - start);
                let rate = step.abs() / dt;
                assert!(follow.yaw().is_finite() && follow.speed().is_finite());
                assert!(
                    rate <= HORIZONTAL_FOLLOW_MAX_SPEED * (1.0 + 1e-12),
                    "start={start:e} offset={offset:e} dt={dt:e} step={step:e} rate={rate:e}"
                );
                assert!(follow.speed() <= HORIZONTAL_FOLLOW_MAX_SPEED * (1.0 + 1e-12));
                assert!(close(follow.speed(), rate, 1e-12));
                let error = wrap_angle(goal - start);
                assert!(step * error >= 0.0 && step.abs() <= error.abs());
                assert_eq!(follow.goal(), Some(goal));
                if dt == 1e-9 && offset.abs() == 1.0 {
                    assert_ne!(
                        follow.yaw(),
                        start,
                        "a representable capped step must advance"
                    );
                }
                follow.update(None, 1000.0);
                assert_eq!(follow.yaw(), goal);
            }
        }
    }
}
