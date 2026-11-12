//! Giro global lejano (candidata): nucleo puro, transform rigido, `World`, render, panel y
//! binario. Fixtures sinteticos salvo la prueba `#[ignore]` del asset real (rutas por entorno).

use umbral::{
    math::{Mat4, Vec3},
    visual::creature_distant_turn::*,
};

const DEG: f64 = std::f64::consts::PI / 180.0;

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// Punto a `length` del centro en la direccion `(yaw, elevation)` del marco de reposo de una
/// criatura con giro `creature_yaw` (yaw positivo hacia +X del OBJ, elevacion hacia +Y).
fn point_at(center: Vec3, creature_yaw: f64, yaw: f64, elevation: f64, length: f64) -> Vec3 {
    let local = Vec3::new(
        yaw.sin() * elevation.cos(),
        elevation.sin(),
        yaw.cos() * elevation.cos(),
    );
    center + Mat4::rotation_y(creature_yaw).transform_dir(local) * length
}

// --- D1: parametros puros ---

#[test]
fn flags_need_the_comparison_and_reject_legacy() {
    assert_eq!(validate_distant_turn_flags(false, false, false), Ok(()));
    assert_eq!(validate_distant_turn_flags(false, false, true), Ok(()));
    assert_eq!(validate_distant_turn_flags(true, true, false), Ok(()));
    assert_eq!(
        validate_distant_turn_flags(true, false, false),
        Err("--creature-distant-turn necesita --creature-eye-compare".into())
    );
    assert_eq!(
        validate_distant_turn_flags(true, true, true),
        Err("--legacy no admite --creature-distant-turn".into())
    );
    assert_eq!(
        validate_distant_turn_flags(true, false, true),
        Err("--legacy no admite --creature-distant-turn".into())
    );
}

#[test]
fn the_distance_weight_is_zero_near_one_far_and_a_smoothstep_between() {
    for d in [0.0, 1.0, 2.5, 3.0] {
        assert_eq!(distance_weight(d), 0.0, "{d} radios");
    }
    for d in [7.0, 8.0, 1e6] {
        assert_eq!(distance_weight(d), 1.0, "{d} radios");
    }
    assert!(close(distance_weight(5.0), 0.5, 1e-15));
    // smoothstep(t) = 3t^2 - 2t^3 con t = (d - 3) / 4.
    assert!(close(distance_weight(4.0), 0.15625, 1e-15));
    assert!(close(distance_weight(6.0), 0.84375, 1e-15));
    // Sin salto en los umbrales y monotono en medio.
    assert!(distance_weight(3.0 + 1e-6) < 1e-11);
    assert!(1.0 - distance_weight(7.0 - 1e-6) < 1e-11);
    let mut previous = 0.0;
    for i in 0..=400 {
        let w = distance_weight(3.0 + i as f64 * 0.01);
        assert!((0.0..=1.0).contains(&w) && w >= previous);
        assert!(w - previous < 0.004, "paso de {} en {i}", w - previous);
        previous = w;
    }
    // Falla cerrada hacia el reposo: sin distancia valida no hay giro.
    for d in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(distance_weight(d), 0.0, "{d}");
    }
}

#[test]
fn turn_angles_stay_inside_the_20_by_12_ellipse() {
    let inside = TurnAngles::clamped(10.0 * DEG, -5.0 * DEG);
    assert_eq!((inside.yaw(), inside.pitch()), (10.0 * DEG, -5.0 * DEG));
    let yaw = TurnAngles::clamped(35.0 * DEG, 0.0);
    assert!(close(yaw.yaw(), DISTANT_TURN_MAX_YAW, 1e-15) && yaw.pitch() == 0.0);
    let pitch = TurnAngles::clamped(0.0, -30.0 * DEG);
    assert!(pitch.yaw() == 0.0 && close(pitch.pitch(), -DISTANT_TURN_MAX_PITCH, 1e-15));
    // Diagonal: en el borde de la elipse y con la direccion normalizada conservada.
    let diagonal = TurnAngles::clamped(40.0 * DEG, 24.0 * DEG);
    let (u, v) = (
        diagonal.yaw() / DISTANT_TURN_MAX_YAW,
        diagonal.pitch() / DISTANT_TURN_MAX_PITCH,
    );
    assert!(close(u.hypot(v), 1.0, 1e-12) && close(u, v, 1e-12) && u > 0.0);
    for (y, p) in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (f64::MAX, f64::MAX)] {
        assert_eq!(TurnAngles::clamped(y, p), TurnAngles::NEUTRAL, "{y} {p}");
    }
    assert!(TurnAngles::NEUTRAL.is_neutral() && !inside.is_neutral());
    assert_eq!(DISTANT_TURN_MAX_YAW, 20.0 * DEG);
    assert_eq!(DISTANT_TURN_MAX_PITCH, 12.0 * DEG);
    assert_eq!(DISTANT_TURN_T95_SECONDS, 8.0);
}

#[test]
fn aim_turn_uses_the_rest_frame_with_up_positive_and_fades_before_turning_around() {
    let center = Vec3::new(30.0, -4.0, -900.0);
    let creature_yaw = 0.7;
    let aim = |yaw: f64, elevation: f64| {
        aim_turn(
            center,
            creature_yaw,
            point_at(center, creature_yaw, yaw, elevation, 640.0),
        )
    };
    // Dentro de la elipse: yaw hacia +X del OBJ y pitch positivo hacia arriba.
    let right = aim(10.0 * DEG, 0.0);
    assert!(close(right.yaw(), 10.0 * DEG, 1e-12) && close(right.pitch(), 0.0, 1e-12));
    let up = aim(0.0, 5.0 * DEG);
    assert!(close(up.yaw(), 0.0, 1e-12) && close(up.pitch(), 5.0 * DEG, 1e-12));
    let down_left = aim(-6.0 * DEG, -4.0 * DEG);
    assert!(down_left.yaw() < 0.0 && down_left.pitch() < 0.0);
    // Fuera: tope en la elipse (25 grados laterales -> 20; 20 grados arriba -> 12).
    let lateral = aim(25.0 * DEG, 0.0);
    assert!(
        close(lateral.yaw(), DISTANT_TURN_MAX_YAW, 1e-12) && close(lateral.pitch(), 0.0, 1e-12)
    );
    let high = aim(0.0, 20.0 * DEG);
    assert!(close(high.pitch(), DISTANT_TURN_MAX_PITCH, 1e-12) && close(high.yaw(), 0.0, 1e-12));
    // Desvanecido 75..105 grados, como el ojo: reduce sin cambiar de signo; detras, reposo.
    let fading = aim(90.0 * DEG, 0.0);
    assert!(
        close(fading.yaw(), 0.5 * DISTANT_TURN_MAX_YAW, 1e-12),
        "{fading:?}"
    );
    for (y, e) in [
        (105.0, 0.0),
        (150.0, 0.0),
        (180.0, 0.0),
        (0.0, 120.0),
        (-170.0, 30.0),
    ] {
        assert_eq!(aim(y * DEG, e * DEG), TurnAngles::NEUTRAL, "{y} {e}");
    }
    // Continuo al pasar por detras-arriba (donde atan2 saltaria de +180 a -180).
    let behind_up = |side: f64| {
        let local = Vec3::new(side, 1.0, -0.2);
        aim_turn(
            center,
            creature_yaw,
            center + Mat4::rotation_y(creature_yaw).transform_dir(local) * 100.0,
        )
    };
    let (a, b) = (behind_up(1e-9), behind_up(-1e-9));
    assert!(close(a.yaw(), b.yaw(), 1e-8) && close(a.pitch(), b.pitch(), 1e-8));
    // Angulo al frente coherente, y sin direccion -> reposo.
    let off = turn_off_axis(
        center,
        creature_yaw,
        point_at(center, creature_yaw, 30.0 * DEG, 0.0, 9.0),
    );
    assert!(close(off.unwrap(), 30.0 * DEG, 1e-12));
    assert_eq!(turn_off_axis(center, creature_yaw, center), None);
    assert_eq!(aim_turn(center, creature_yaw, center), TurnAngles::NEUTRAL);
    let bad = Vec3::new(f64::NAN, 0.0, 0.0);
    assert_eq!(aim_turn(center, creature_yaw, bad), TurnAngles::NEUTRAL);
    assert_eq!(aim_turn(bad, creature_yaw, center), TurnAngles::NEUTRAL);
    assert_eq!(
        aim_turn(center, f64::INFINITY, Vec3::ZERO),
        TurnAngles::NEUTRAL
    );
    // Distancias enormes o diminutas no desbordan.
    let tiny = aim_turn(Vec3::ZERO, 0.0, Vec3::new(1e-300, 0.0, 1e-300));
    let huge = aim_turn(Vec3::ZERO, 0.0, Vec3::new(1e300, 0.0, 1e300));
    assert!(close(tiny.yaw(), DISTANT_TURN_MAX_YAW, 1e-12) && tiny == huge);
}

// --- D2: estado temporal ---

fn angles_close(a: TurnAngles, b: TurnAngles, tolerance: f64) -> bool {
    close(a.yaw(), b.yaw(), tolerance) && close(a.pitch(), b.pitch(), tolerance)
}

#[test]
fn the_turn_starts_on_and_neutral_and_eases_with_t95_of_eight_real_seconds() {
    let turn = DistantTurn::new();
    assert!(turn.is_active());
    assert_eq!(turn.current(), TurnAngles::NEUTRAL);
    assert_eq!(DistantTurn::default(), turn);
    let goal = TurnAngles::clamped(18.0 * DEG, -6.0 * DEG);
    // El 95 % del cambio en 8 s, sin pasarse y dentro de la elipse en cada paso.
    let mut stepped = turn;
    for _ in 0..480 {
        stepped.update(goal, 1.0 / 60.0);
        let c = stepped.current();
        assert!(c.yaw() <= goal.yaw() && c.yaw() >= 0.0 && c.pitch() >= goal.pitch());
        assert_eq!(TurnAngles::clamped(c.yaw(), c.pitch()), c);
    }
    let remaining = (goal.yaw() - stepped.current().yaw()) / goal.yaw();
    assert!(close(remaining, 0.05, 1e-9), "{remaining}");
    // Independiente de como se trocee el tiempo real.
    let mut once = turn;
    once.update(goal, 8.0);
    let mut uneven = turn;
    for dt in [0.5, 3.0, 0.001, 2.499, 2.0] {
        uneven.update(goal, dt);
    }
    assert!(angles_close(once.current(), stepped.current(), 1e-12));
    assert!(angles_close(once.current(), uneven.current(), 1e-12));
    // Tras 1 s queda el 68.8 % del cambio (20^(-1/8)): lento, no un salto.
    let mut one = turn;
    one.update(goal, 1.0);
    assert!(close(
        1.0 - one.current().yaw() / goal.yaw(),
        20f64.powf(-1.0 / 8.0),
        1e-12
    ));
    // Llega exacto al objetivo y al reposo; elapsed invalido no avanza.
    let mut long = turn;
    long.update(goal, 200.0);
    assert_eq!(long.current(), goal);
    for dt in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut same = one;
        same.update(TurnAngles::NEUTRAL, dt);
        assert_eq!(same, one, "{dt}");
    }
    long.update(TurnAngles::NEUTRAL, 200.0);
    assert_eq!(long.current(), TurnAngles::NEUTRAL);
    // `reset` vuelve al reposo exacto y conserva encendido/apagado.
    let mut off = one;
    off.set_active(false);
    off.reset();
    assert_eq!(off.current(), TurnAngles::NEUTRAL);
    assert!(!off.is_active());
}

#[test]
fn the_goal_is_the_aim_times_the_distance_weight_and_off_returns_smoothly() {
    // Dentro de la elipse (0.8, 0.33): la mitad es exactamente (8, 2) grados.
    let aim = TurnAngles::clamped(16.0 * DEG, 4.0 * DEG);
    let mut turn = DistantTurn::new();
    assert_eq!(turn.goal(aim, 1.0), aim);
    assert_eq!(turn.goal(aim, 0.0), TurnAngles::NEUTRAL);
    let half = turn.goal(aim, 0.5);
    assert!(angles_close(
        half,
        TurnAngles::clamped(8.0 * DEG, 2.0 * DEG),
        1e-15
    ));
    for w in [f64::NAN, -0.5] {
        assert_eq!(turn.goal(aim, w), TurnAngles::NEUTRAL, "{w}");
    }
    assert_eq!(turn.goal(aim, 3.0), aim);
    // Apagado: objetivo neutral siempre, pero el estado no salta: vuelve con la misma
    // persecucion (el texto del panel avisa del retorno).
    turn.update(aim, 200.0);
    turn.set_active(false);
    assert_eq!(turn.current(), aim);
    assert_eq!(turn.goal(aim, 1.0), TurnAngles::NEUTRAL);
    let before = turn.current();
    turn.update(turn.goal(aim, 1.0), 1.0 / 60.0);
    let step = before.yaw() - turn.current().yaw();
    assert!(step > 0.0 && step < 0.01 * before.yaw(), "{step}");
    turn.set_active(true);
    assert_eq!(turn.goal(aim, 1.0), aim);
}

#[test]
fn crossing_the_distance_thresholds_never_jumps() {
    // Camara que se aleja de 2 a 8 radios y vuelve, a 60 Hz: el objetivo es continuo en la
    // distancia y el estado avanza como mucho lo que permite la persecucion en un cuadro.
    let aim = TurnAngles::clamped(30.0 * DEG, 0.0);
    let mut turn = DistantTurn::new();
    let dt = 1.0 / 60.0;
    let rate = 20f64.ln() / DISTANT_TURN_T95_SECONDS * dt;
    let (mut previous_goal, mut previous) = (TurnAngles::NEUTRAL, turn.current());
    let distances = (0..=720)
        .map(|i| 2.0 + 6.0 * i as f64 / 720.0)
        .chain((0..=720).map(|i| 8.0 - 6.0 * i as f64 / 720.0));
    for d in distances {
        let goal = turn.goal(aim, distance_weight(d));
        // Pendiente maxima del smoothstep: 1.5 / 4 por radio, por 6 / 720 radios por muestra.
        let slope = 1.5 / 4.0 * (6.0 / 720.0);
        assert!((goal.yaw() - previous_goal.yaw()).abs() <= slope * DISTANT_TURN_MAX_YAW + 1e-15);
        turn.update(goal, dt);
        let c = turn.current();
        assert!((c.yaw() - previous.yaw()).abs() <= rate * DISTANT_TURN_MAX_YAW + 1e-15);
        (previous_goal, previous) = (goal, c);
    }
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

/// Cupula con un triangulo de cuerpo desplazado: bounds descentrados como en el asset real.
fn offset_dome() -> Model {
    parse_obj(
        &(dome_source()
            + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n"),
    )
    .unwrap()
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

// --- D3: transform rigido alrededor del centro fijo ---

fn mat_close(a: &Mat4, b: &Mat4, tolerance: f64) -> bool {
    a.m.iter()
        .flatten()
        .zip(b.m.iter().flatten())
        .all(|(x, y)| close(*x, *y, tolerance))
}

fn vec_close(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    (a - b).length() <= tolerance
}

#[test]
fn the_rigid_turn_keeps_the_center_fixed_with_offset_bounds_and_placement_yaw() {
    let model = offset_dome();
    let bounds_center = Vec3::from_array(model.bounds_center);
    assert!(
        bounds_center.length() > 0.1,
        "fixture con bounds descentrados"
    );
    let creature = StaticCreature::new(model, texture(), turned_placement()).unwrap();
    let placement = creature.placement();
    let (center, py) = (placement.center(), placement.yaw());
    let base = creature.transform();
    // Reposo: exactamente el transform y el objetivo de siempre (sin redondeo nuevo).
    assert_eq!(turned_transform(&creature, TurnAngles::NEUTRAL), base);
    let target = Vec3::new(123.25, -7.5, 310.125);
    assert_eq!(
        rest_frame_target(center, py, TurnAngles::NEUTRAL, target),
        target
    );
    let angles = TurnAngles::clamped(17.0 * DEG, 9.0 * DEG);
    let turned = turned_transform(&creature, angles);
    // Mismo escalado; Euler explicito (x = -pitch, y = yaw de la criatura + yaw, z = 0).
    assert_eq!(turned.scale, base.scale);
    assert_eq!(
        turned_euler(py, angles),
        Vec3::new(-angles.pitch(), py + angles.yaw(), 0.0)
    );
    assert_eq!(turned.rotation, turned_euler(py, angles));
    // El centro envolvente del modelo sigue en el centro de la colocacion.
    let m = turned.matrix();
    assert!(vec_close(m.transform_point(bounds_center), center, 1e-9));
    // Cada vertice: el de reposo girado alrededor del centro (nada orbita).
    let g = world_turn(py, angles);
    for v in &creature.model().vertices {
        let p = Vec3::from_array(*v);
        let rest = base.matrix().transform_point(p);
        let expected = center + g.transform_dir(rest - center);
        assert!(vec_close(m.transform_point(p), expected, 1e-9));
        assert!(close(
            (m.transform_point(p) - center).length(),
            (rest - center).length(),
            1e-9
        ));
    }
    // Normales rigidas con el mismo giro.
    for n in &creature.model().normals {
        let n = Vec3::from_array(*n);
        let expected = g.transform_dir(base.transform_normal(n));
        assert!(vec_close(turned.transform_normal(n), expected, 1e-12));
    }
    // Composicion: G * Ry(py) = Ry(py + yaw) * Rx(-pitch); inversa analitica exacta.
    assert!(mat_close(
        &(g * Mat4::rotation_y(py)),
        &Mat4::from_euler(turned_euler(py, angles)),
        1e-15
    ));
    let inverse = world_turn_inverse(py, angles);
    assert!(mat_close(&(inverse * g), &Mat4::IDENTITY, 1e-15));
    assert!(mat_close(&(g * inverse), &Mat4::IDENTITY, 1e-15));
    // Convencion: yaw positivo lleva el frente hacia +X del OBJ y pitch positivo hacia arriba.
    let front = g.transform_dir(placement.front());
    let expected_front = Mat4::rotation_y(py).transform_dir(Vec3::new(
        angles.yaw().sin() * angles.pitch().cos(),
        angles.pitch().sin(),
        angles.yaw().cos() * angles.pitch().cos(),
    ));
    assert!(vec_close(front, expected_front, 1e-15) && front.y > 0.0);
    // El objetivo en el marco de reposo deshace el giro: ida y vuelta exacta.
    let rest = rest_frame_target(center, py, angles, target);
    assert!(vec_close(
        center + g.transform_dir(rest - center),
        target,
        1e-9
    ));
    assert!(close(
        (rest - center).length(),
        (target - center).length(),
        1e-9
    ));
    assert!(!vec_close(rest, target, 1.0));
}

// --- D4: ciclo de vida en `World` y presets ---

use umbral::{
    assets::{obj::load_model, ship_model_path},
    scene::world::World,
    visual::{
        creature_eye_comparison::EyeTarget, creature_eye_follow::EyeFollowSettings,
        staged_creature_reveal::StagedRevealSettings, static_creature::SPAWN_DISTANCE_RADII,
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

/// Mundo con la criatura girada, etapas y mirada a la camara (como la comparacion), sin giro
/// global y con la nave en el preset cercano de siempre.
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

fn turn_world() -> World {
    let mut world = eye_world();
    world.enable_creature_distant_turn().unwrap();
    world
}

#[test]
fn the_turn_needs_the_eye_and_lives_and_dies_with_it() {
    let message = "el giro global necesita --creature-eye-compare (mirada activa)";
    let mut plain = new_world();
    assert_eq!(plain.enable_creature_distant_turn(), Err(message.into()));
    plain.set_creature(turned_creature()).unwrap();
    plain
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
    assert_eq!(plain.enable_creature_distant_turn(), Err(message.into()));
    assert_eq!(plain.creature_distant_turn(), None);
    assert!(plain.set_creature_distant_turn_active(false).is_err());

    let mut world = turn_world();
    assert_eq!(world.creature_distant_turn(), Some(DistantTurn::new()));
    // H: solo cambia encendido/apagado; sin giro, `Err` sin efectos.
    world.set_creature_distant_turn_active(false).unwrap();
    assert!(!world.creature_distant_turn().unwrap().is_active());
    world.set_creature_distant_turn_active(true).unwrap();
    assert!(world.creature_distant_turn().unwrap().is_active());
    world.disable_creature_distant_turn();
    assert_eq!(world.creature_distant_turn(), None);
    assert!(world.set_creature_distant_turn_active(true).is_err());
    assert!(world.creature_eye_follow().is_some(), "la mirada sigue");
    // Apagar la mirada (o reiniciar/cambiar el modo) lo quita; reactivarla no lo trae.
    for reset in 0..3 {
        let mut world = turn_world();
        match reset {
            0 => world.disable_creature_eye_follow(),
            1 => world
                .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
                .unwrap(),
            _ => world.disable_staged_creature_reveal(),
        }
        assert_eq!(world.creature_distant_turn(), None, "{reset}");
        if reset < 2 {
            world
                .enable_creature_eye_follow(EyeFollowSettings::default())
                .unwrap();
            assert_eq!(world.creature_distant_turn(), None, "{reset}");
        }
    }
}

#[test]
fn presets_place_the_ship_reset_the_turn_and_snap_the_eye_against_the_placed_camera() {
    let placement = turned_placement();
    let (near, near_forward) = preset_pose(&placement, CreaturePreset::Near);
    assert_eq!((near, near_forward), placement.spawn_pose());
    let (far, far_forward) = preset_pose(&placement, CreaturePreset::Far);
    let radius = placement.world_radius();
    assert!(close(
        (far - placement.center()).length(),
        FAR_PRESET_RADII * radius,
        1e-9
    ));
    assert!(vec_close(
        far_forward,
        (placement.center() - far).normalized(),
        1e-12
    ));
    assert!(close(far.y, placement.center().y, 1e-12), "sin elevacion");
    // 25 grados laterales hacia +X del OBJ (yaw positivo del marco de reposo).
    let off = turn_off_axis(placement.center(), placement.yaw(), far).unwrap();
    assert!(close(off, FAR_PRESET_LATERAL, 1e-12));
    assert!(aim_turn(placement.center(), placement.yaw(), far).yaw() > 0.0);
    assert_eq!(FAR_PRESET_RADII, 8.0);
    assert!(close(FAR_PRESET_LATERAL, 25.0 * DEG, 1e-15));
    assert_eq!(SPAWN_DISTANCE_RADII, 2.5);

    let mut world = turn_world();
    world.debug_oxygen01 = 0.25;
    world.set_creature_distant_turn_active(false).unwrap();
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    assert!(vec_close(world.ship.position, far, 1e-9));
    assert!(vec_close(world.ship.forward(), far_forward, 1e-9));
    // Giro en reposo exacto; O2, objetivo de la mirada y H se conservan.
    let turn = world.creature_distant_turn().unwrap();
    assert_eq!(turn.current(), TurnAngles::NEUTRAL);
    assert!(!turn.is_active());
    assert_eq!(world.debug_oxygen01, 0.25);
    assert_eq!(world.creature_eye_target(), Some(EyeTarget::Camera));
    // Ojo en su objetivo contra la camara ya colocada (sin transicion).
    let creature = world.creature().unwrap();
    let skin = world.creature_eye_skin().unwrap();
    let goal = skin.aim(creature, world.camera.position);
    assert_eq!(world.creature_eye_follow().unwrap().current(), goal);
    assert!(!goal.is_neutral());
    // Near: el arranque `--near-creature` de siempre.
    world.place_creature_preset(CreaturePreset::Near).unwrap();
    assert_eq!(world.ship.position, near);
    let mut reference = eye_world();
    reference.place_near_creature().unwrap();
    assert_eq!(world.camera.position, reference.camera.position);
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        reference.creature_eye_follow().unwrap().current()
    );
    // Sin criatura: `Err` y nada cambia.
    let before = world.ship.position;
    world.clear_creature();
    assert!(world.place_creature_preset(CreaturePreset::Far).is_err());
    assert_eq!(world.ship.position, before);
}

// --- D5: el giro avanza con segundos reales hacia el punto elegido y el ojo compensa ---

use umbral::scene::ship::ShipInput;

fn far_world() -> World {
    let mut world = turn_world();
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    world
}

/// Lo que deberia pedir el ojo: la calibracion de reposo hacia el punto llevado al marco de
/// reposo por la inversa del giro actual.
fn compensated_goal(world: &World, point: Vec3) -> GazeAngles {
    let creature = world.creature().unwrap();
    let placement = creature.placement();
    let turn = world.creature_distant_turn().unwrap().current();
    world.creature_eye_skin().unwrap().aim(
        creature,
        rest_frame_target(placement.center(), placement.yaw(), turn, point),
    )
}

use umbral::visual::creature_eye_follow::GazeAngles;

#[test]
fn far_away_the_whole_creature_eases_toward_the_chosen_point_and_the_eye_compensates() {
    let mut world = far_world();
    let placement = turned_placement();
    let (center, py) = (placement.center(), placement.yaw());
    let status = world.creature_distant_turn_status().unwrap();
    let distance = (world.camera.position - center).length() / placement.world_radius();
    assert!(distance > 8.0 && distance < 8.5, "{distance}");
    assert_eq!(status.distance_radii, Some(distance));
    assert_eq!(status.weight, Some(1.0));
    // Pedido hacia la POSICION de la camara (objetivo G = CAMARA), recortado a 20 grados.
    let aim = aim_turn(center, py, world.camera.position);
    assert_eq!(status.aim, Some(aim));
    assert_eq!(status.goal, Some(aim));
    // En el borde de la elipse (tope): casi todo yaw y un poco de pitch positivo, porque la
    // camara va 6 unidades por encima de la nave.
    let edge = (aim.yaw() / DISTANT_TURN_MAX_YAW).hypot(aim.pitch() / DISTANT_TURN_MAX_PITCH);
    assert!(close(edge, 1.0, 1e-12), "{aim:?}");
    assert!(aim.yaw() > 19.9 * DEG && aim.pitch() > 0.0, "{aim:?}");
    assert_eq!(
        status.off_axis,
        turn_off_axis(center, py, world.camera.position)
    );
    assert_ne!(aim, aim_turn(center, py, world.camera.target));
    // Avanza solo con `update_creature_eye_follow` (segundos reales); F5 no lo congela.
    world.toggles.freeze_orbits = true;
    world.update(&ShipInput::default(), 0.1);
    assert_eq!(
        world.creature_distant_turn().unwrap().current(),
        TurnAngles::NEUTRAL
    );
    world.update_creature_eye_follow(8.0);
    let current = world.creature_distant_turn().unwrap().current();
    assert!(close(current.yaw() / aim.yaw(), 0.95, 1e-9), "{current:?}");
    // El ojo apunta al punto compensado, no al de la criatura quieta.
    let compensated = compensated_goal(&world, world.camera.position);
    assert_eq!(world.creature_eye_goal(), Some(compensated));
    let still = world
        .creature_eye_skin()
        .unwrap()
        .aim(world.creature().unwrap(), world.camera.position);
    assert_ne!(compensated, still);
    assert!(
        compensated.yaw().abs() < still.yaw().abs(),
        "el giro absorbe parte"
    );
    // Y el estado del ojo persigue ese objetivo (1.5 s), despues del giro del mismo paso.
    world.update_creature_eye_follow(30.0);
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        compensated_goal(&world, world.camera.position)
    );
    // G = NAVE: el giro apunta a la posicion de la nave.
    world.set_creature_eye_target(EyeTarget::Ship).unwrap();
    let status = world.creature_distant_turn_status().unwrap();
    assert_eq!(status.aim, Some(aim_turn(center, py, world.ship.position)));
}

#[test]
fn near_from_rest_the_approved_gaze_is_exact_and_h_off_eases_back() {
    // Cerca (2.5 radios, camara a menos de 3): peso 0, el giro no se mueve y la mirada es
    // bit a bit la de la comparacion sin giro, en una trayectoria pilotada.
    let mut turned = turn_world();
    turned.place_creature_preset(CreaturePreset::Near).unwrap();
    let mut plain = eye_world();
    let input = ShipInput {
        thrust: 0.3,
        yaw: 0.4,
        pitch: -0.2,
        ..ShipInput::default()
    };
    for frame in 0..90 {
        for world in [&mut turned, &mut plain] {
            world.update(&input, 1.0 / 60.0);
            world.update_creature_eye_follow(1.0 / 60.0);
        }
        let status = turned.creature_distant_turn_status().unwrap();
        assert!(status.distance_radii.unwrap() < 3.0, "{frame}");
        assert_eq!(status.weight, Some(0.0));
        assert_eq!(status.turn.current(), TurnAngles::NEUTRAL);
        assert_eq!(
            turned.creature_eye_follow(),
            plain.creature_eye_follow(),
            "{frame}"
        );
        assert_eq!(
            turned.creature_eye_goal(),
            plain.creature_eye_goal(),
            "{frame}"
        );
    }
    // H apagado lejos: vuelve suave con el mismo estado temporal (sin salto) y llega al
    // reposo exacto; el ojo vuelve al objetivo sin compensar.
    let mut world = far_world();
    world.update_creature_eye_follow(20.0);
    let turned_yaw = world.creature_distant_turn().unwrap().current().yaw();
    world.set_creature_distant_turn_active(false).unwrap();
    let status = world.creature_distant_turn_status().unwrap();
    assert_eq!(status.goal, Some(TurnAngles::NEUTRAL));
    assert_eq!(status.weight, Some(1.0));
    world.update_creature_eye_follow(1.0);
    let after = world.creature_distant_turn().unwrap().current().yaw();
    assert!(close(after / turned_yaw, 20f64.powf(-1.0 / 8.0), 1e-9));
    world.update_creature_eye_follow(200.0);
    assert_eq!(
        world.creature_distant_turn().unwrap().current(),
        TurnAngles::NEUTRAL
    );
    let still = world
        .creature_eye_skin()
        .unwrap()
        .aim(world.creature().unwrap(), world.camera.position);
    assert_eq!(world.creature_eye_goal(), Some(still));
}

#[test]
fn creature_changes_keep_the_turn_and_snap_the_eye_against_the_compensated_point() {
    let mut world = far_world();
    world.update_creature_eye_follow(5.0);
    let turn = world.creature_distant_turn().unwrap();
    assert!(!turn.current().is_neutral());
    // Sin criatura: sin pedido ni avance; el estado se conserva.
    world.clear_creature();
    let status = world.creature_distant_turn_status().unwrap();
    assert_eq!((status.aim, status.goal, status.weight), (None, None, None));
    world.update_creature_eye_follow(5.0);
    assert_eq!(world.creature_distant_turn(), Some(turn));
    // Con criatura de nuevo: mismo giro y el ojo salta al objetivo compensado.
    world.set_creature(turned_creature()).unwrap();
    assert_eq!(world.creature_distant_turn(), Some(turn));
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        compensated_goal(&world, world.camera.position)
    );
    assert_eq!(
        world.creature_eye_goal(),
        Some(compensated_goal(&world, world.camera.position))
    );
}

// --- D6: rutas de render ---

use umbral::{
    renderer::{framebuffer::Framebuffer, stats::RenderStats},
    scene::world::SPACE_COLOR,
};

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Mundo lejano con O2 bajo (criatura opaca) y el giro ya avanzado `seconds`.
fn turned_far_world(oxygen: f64, seconds: f64) -> World {
    let mut world = far_world();
    world.debug_oxygen01 = oxygen;
    world.snap_creature_reveal();
    world.update_creature_eye_follow(seconds);
    world
}

#[test]
fn a_neutral_turn_draws_exactly_the_old_paths() {
    for (oxygen, preset) in [
        (0.05, CreaturePreset::Near),
        (0.25, CreaturePreset::Near),
        (0.05, CreaturePreset::Far),
        (0.25, CreaturePreset::Far),
        (1.0, CreaturePreset::Far),
    ] {
        let mut with_turn = turn_world();
        let mut without = eye_world();
        for world in [&mut with_turn, &mut without] {
            world.debug_oxygen01 = oxygen;
            world.place_creature_preset(preset).unwrap();
        }
        assert_eq!(
            with_turn.creature_distant_turn().unwrap().current(),
            TurnAngles::NEUTRAL
        );
        let (a, b) = (render(&with_turn), render(&without));
        assert!(a.0 == b.0, "{oxygen} {preset:?}");
        assert_eq!(a.1, b.1, "{oxygen} {preset:?}");
        assert_eq!(with_turn.creature_eye_posed(), without.creature_eye_posed());
    }
}

#[test]
fn a_turned_creature_draws_both_parts_turned_even_with_a_neutral_eye() {
    for oxygen in [0.05, 0.25] {
        let world = turned_far_world(oxygen, 20.0);
        assert!(
            !world
                .creature_distant_turn()
                .unwrap()
                .current()
                .is_neutral()
        );
        let mut still = turned_far_world(oxygen, 20.0);
        still.disable_creature_distant_turn();
        let (turned, turned_stats) = render(&world);
        let (rest, rest_stats) = render(&still);
        assert!(turned != rest, "el giro se ve ({oxygen})");
        assert_eq!(turned_stats.objects_submitted, rest_stats.objects_submitted);
        assert_eq!(
            turned_stats.triangles_submitted,
            rest_stats.triangles_submitted
        );
    }
    // Ojo exactamente en reposo con el giro aun lejos del reposo: la nave pasa detras de la
    // criatura (G = NAVE); el ojo llega a su reposo exacto en 8 s y al giro le queda el 5 %.
    let mut world = turned_far_world(0.05, 20.0);
    world.set_creature_eye_target(EyeTarget::Ship).unwrap();
    let placement = turned_placement();
    world.ship.position = placement.center() - placement.front() * 300.0;
    world.update_creature_eye_follow(8.0);
    assert!(world.creature_eye_follow().unwrap().current().is_neutral());
    let turn = world.creature_distant_turn().unwrap().current();
    assert!(turn.yaw() > 0.5 * DEG, "{turn:?}");
    let mut still = turned_far_world(0.05, 20.0);
    still.set_creature_eye_target(EyeTarget::Ship).unwrap();
    still.ship.position = world.ship.position;
    still.update_creature_eye_follow(8.0);
    still.disable_creature_distant_turn();
    assert!(still.creature_eye_follow().unwrap().current().is_neutral());
    let ((turned, _), (rest, _)) = (render(&world), render(&still));
    assert!(turned != rest, "giro global con el ojo en reposo");
}

#[test]
fn culling_and_hidden_oxygen_never_pose_and_count_like_the_static_creature() {
    // Mirando hacia fuera: descartada con la esfera completa, sin vertices ni pose.
    let mut world = turned_far_world(0.05, 20.0);
    let center = turned_placement().center();
    world.camera.target = world.camera.position + (world.camera.position - center);
    let poses = world.creature_eye_pose_updates();
    let (_, stats) = render(&world);
    let faces = world.creature().unwrap().model().faces.len();
    let ship = {
        let mut empty = turned_far_world(0.05, 20.0);
        empty.camera.target = world.camera.target;
        empty.clear_creature();
        render(&empty).1
    };
    assert_eq!(stats.objects_culled, ship.objects_culled + 1);
    assert_eq!(stats.triangles_culled, ship.triangles_culled + faces);
    assert_eq!(stats.vertices_processed, ship.vertices_processed);
    assert_eq!(world.creature_eye_pose_updates(), poses);
    // O2 alto (oculta): ni se dibuja ni se posa.
    let mut hidden = turned_far_world(1.0, 20.0);
    let poses = hidden.creature_eye_pose_updates();
    hidden.update_creature_eye_follow(0.5);
    let (pixels, stats) = render(&hidden);
    let mut empty = turned_far_world(1.0, 20.0);
    empty.update_creature_eye_follow(0.5);
    empty.clear_creature();
    let (empty_pixels, empty_stats) = render(&empty);
    assert!(pixels == empty_pixels);
    assert_eq!(stats, empty_stats);
    assert_eq!(hidden.creature_eye_pose_updates(), poses);
}

// --- D7: panel del giro y texto del ojo en el marco compensado ---

use umbral::visual::creature_eye_comparison::{
    eye_comparison_lines, eye_comparison_panel, off_axis_degrees,
};

fn ascii_for_the_font(line: &str) -> bool {
    line.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || " -.:/%()=+,|".contains(c))
}

fn status_with(turn: DistantTurn, aim: TurnAngles, weight: f64, off: f64) -> DistantTurnStatus {
    DistantTurnStatus {
        turn,
        aim: Some(aim),
        goal: Some(turn.goal(aim, weight)),
        distance_radii: Some(8.2753),
        weight: Some(weight),
        off_axis: Some(off * DEG),
    }
}

#[test]
fn the_turn_panel_text_shows_h_presets_weight_angles_and_the_limit_state() {
    let aim = TurnAngles::clamped(30.0 * DEG, 0.6 * DEG);
    let mut turn = DistantTurn::new();
    turn.update(aim, 2.0);
    let on = status_with(turn, aim, 1.0, 25.3);
    let lines = distant_turn_lines(&on, CreaturePreset::Far);
    let c = turn.current();
    assert_eq!(
        lines,
        [
            "GIRO GLOBAL: ON   H APAGA   T95 8 S".to_string(),
            "7 CERCA  8 LEJOS  R REPITE LEJOS".to_string(),
            "CAMARA A 8.3 RADIOS  PESO 1.00 (3-7)".to_string(),
            format!(
                "GLOBAL ACTUAL   YAW {:+.1}  PITCH {:+.1}",
                c.yaw().to_degrees(),
                c.pitch().to_degrees()
            ),
            format!(
                "GLOBAL OBJETIVO YAW {:+.1}  PITCH {:+.1}",
                aim.yaw().to_degrees(),
                aim.pitch().to_degrees()
            ),
            "LIMITE 20/12: TOPE  FRENTE 25.3 GRADOS".to_string(),
        ]
    );
    assert_eq!(lines[4], "GLOBAL OBJETIVO YAW +20.0  PITCH +0.4");
    // Apagado: lo dice y avisa del retorno suave; el objetivo es el reposo.
    let mut off = turn;
    off.set_active(false);
    let lines = distant_turn_lines(&status_with(off, aim, 1.0, 25.3), CreaturePreset::Near);
    assert_eq!(lines[0], "GIRO GLOBAL: OFF  H ENCIENDE (VUELVE SUAVE)");
    assert_eq!(lines[1], "7 CERCA  8 LEJOS  R REPITE CERCA");
    assert_eq!(lines[4], "GLOBAL OBJETIVO YAW +0.0  PITCH +0.0");
    // Estados del pedido (antes del peso): dentro, desvanecido y reposo.
    let small = TurnAngles::clamped(5.0 * DEG, -2.0 * DEG);
    let near = status_with(DistantTurn::new(), small, 0.0, 5.4);
    let near_lines = distant_turn_lines(&near, CreaturePreset::Near);
    assert_eq!(near_lines[2], "CAMARA A 8.3 RADIOS  PESO 0.00 (3-7)");
    assert_eq!(near_lines[5], "LIMITE 20/12: DENTRO  FRENTE 5.4 GRADOS");
    let fading = status_with(DistantTurn::new(), small, 1.0, 90.0);
    assert!(
        distant_turn_lines(&fading, CreaturePreset::Far)[5].starts_with("LIMITE 20/12: DESVANECE")
    );
    let rest = status_with(DistantTurn::new(), TurnAngles::NEUTRAL, 1.0, 140.0);
    assert!(distant_turn_lines(&rest, CreaturePreset::Far)[5].starts_with("LIMITE 20/12: REPOSO"));
    let none = DistantTurnStatus {
        aim: None,
        goal: None,
        distance_radii: None,
        weight: None,
        off_axis: None,
        ..rest
    };
    let none_lines = distant_turn_lines(&none, CreaturePreset::Far);
    assert_eq!(none_lines[2..], ["SIN CRIATURA: SIN GIRO".to_string()]);
    for status in [on, near, fading, rest, none] {
        for line in distant_turn_lines(&status, CreaturePreset::Far) {
            assert!(ascii_for_the_font(&line) && line.len() <= 44, "{line}");
        }
    }
}

#[test]
fn the_turn_panel_sits_bottom_right_beside_the_comparison_or_above_it_when_narrow() {
    let mut turn = DistantTurn::new();
    turn.set_active(false);
    let wide = DistantTurnStatus {
        turn,
        aim: Some(TurnAngles::clamped(-1.0, -1.0)),
        goal: Some(TurnAngles::NEUTRAL),
        distance_radii: Some(1234.56),
        weight: Some(1.0),
        off_axis: Some(104.9 * DEG),
    };
    let lines = distant_turn_lines(&wide, CreaturePreset::Near);
    let compare = vec![
        "MIRADA A: CAMARA   G CAMBIA A NAVE".to_string(),
        "LIMITE 10/8: DESVANECE   EJE 104.9 GRADOS (FADE 75-105)".to_string(),
        "OBJETIVO YAW -10.0  PITCH -8.0".to_string(),
        "ACTUAL   YAW -10.0  PITCH -8.0".to_string(),
    ];
    for (w, h, beside) in [(320, 180, false), (640, 360, true), (1280, 720, true)] {
        let left = eye_comparison_panel(w, h, &compare);
        let panel = distant_turn_panel(w, h, &lines, &left);
        assert_eq!(panel.scale, left.scale, "{w}");
        assert!(
            panel.x + panel.w <= w && panel.y + panel.h <= h,
            "{w}: {panel:?}"
        );
        let overlap = panel.x < left.x + left.w
            && left.x < panel.x + panel.w
            && panel.y < left.y + left.h
            && left.y < panel.y + panel.h;
        assert!(!overlap, "{w}: {panel:?} {left:?}");
        if beside {
            assert_eq!((panel.x + panel.w, panel.y + panel.h), (w, h), "{w}");
        } else {
            assert_eq!((panel.x, panel.y + panel.h), (0, left.y), "{w}");
        }
        // Dibuja solo dentro de su rectangulo (con fondo blanco, oscurecido y texto).
        let mut fb = Framebuffer::new(w, h, 0xFFFFFF).unwrap();
        draw_distant_turn_panel(&mut fb, &lines, &left);
        let mut touched = 0;
        for y in 0..h {
            for x in 0..w {
                let inside = (panel.x..panel.x + panel.w).contains(&x)
                    && (panel.y..panel.y + panel.h).contains(&y);
                let changed = fb.pixels[y * w + x] != 0xFFFFFF;
                assert!(inside || !changed, "{w}: ({x}, {y})");
                touched += usize::from(changed);
            }
        }
        assert_eq!(touched, panel.w * panel.h, "{w}");
    }
}

#[test]
fn the_eye_panel_reports_the_compensated_frame() {
    let mut world = far_world();
    world.update_creature_eye_follow(20.0);
    let status = world.creature_eye_comparison_status().unwrap();
    let creature = world.creature().unwrap();
    let placement = creature.placement();
    let turn = world.creature_distant_turn().unwrap().current();
    let rest = rest_frame_target(
        placement.center(),
        placement.yaw(),
        turn,
        world.camera.position,
    );
    let skin = world.creature_eye_skin().unwrap();
    assert_eq!(status.goal, Some(skin.aim(creature, rest)));
    let pivot = creature.transform().matrix().transform_point(skin.pivot());
    let compensated = off_axis_degrees(pivot, skin.optical_axis(), placement.yaw(), rest);
    assert_eq!(status.off_axis_degrees, compensated);
    let still = off_axis_degrees(
        pivot,
        skin.optical_axis(),
        placement.yaw(),
        world.camera.position,
    );
    assert!(
        (compensated.unwrap() - still.unwrap()).abs() > 1.0,
        "{compensated:?} {still:?}"
    );
    // El texto del ojo sale de ese estado (sin cambios en el helper).
    assert_eq!(eye_comparison_lines(&status).len(), 4);
}

// --- D8: linea de comandos, entrada de biblioteca y arranque de la candidata ---

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::{
    app::{GameOptions, run_game_with_distant_turn},
    visual::{
        creature_eye_comparison::draw_eye_comparison_panel,
        staged_creature_reveal::CREATURE_EYE_REGION, static_creature::CreatureOptions,
    },
};

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("umbral-distant-turn-{name}-{}", std::process::id()));
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
    let source = dome_source()
        + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n";
    let mut out = String::new();
    for line in source.lines() {
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
    let cases = [
        (
            missing(&[
                "--creature-reveal-stages",
                "--creature-eye-follow",
                "--creature-distant-turn",
            ]),
            "--creature-distant-turn necesita --creature-eye-compare",
        ),
        (
            missing(&[
                "--creature-reveal-stages",
                "--creature-eye-compare",
                "--creature-distant-turn",
            ]),
            "--creature-eye-compare necesita --creature-eye-follow",
        ),
        (
            missing(&[
                "--creature-reveal",
                "--creature-reveal-stages",
                "--creature-eye-follow",
                "--creature-eye-compare",
                "--creature-distant-turn",
            ]),
            "incompatibles",
        ),
        (
            args(&[
                o("--legacy"),
                o("--creature-distant-turn"),
                o("--output"),
                output.as_os_str(),
            ]),
            "--legacy no admite --creature-distant-turn",
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
    let alias = args(&[
        o("--creature-model"),
        model.as_os_str(),
        o("--creature-texture"),
        texture_path.as_os_str(),
        o("--creature-reveal-stages"),
        o("--creature-eye-follow"),
        o("--creature-eye-compare"),
        o("--creature-distant-turn"),
        o("--output"),
        texture_path.as_os_str(),
    ]);
    let out = run_game(&alias);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("coincide con la entrada"));
    assert_eq!(std::fs::read(&texture_path).unwrap(), texture_bytes);
    // La ayuda lo documenta.
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.contains("--creature-distant-turn (requiere --creature-eye-compare)"),
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
        let error = run_game_with_distant_turn(o).expect_err(needle);
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
        assert!(!output.exists() && !model.exists() && !texture_path.exists());
    }
    // Alias con entradas reales (el guard actua si la salida existe): sin tocarlas.
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
    let error = run_game_with_distant_turn(alias).expect_err("alias");
    assert!(error.contains("coincide con la entrada"), "{error}");
    assert_eq!(std::fs::read(&real_texture).unwrap(), before);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_game_starts_the_candidate_far_with_both_panels_in_headless() {
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
        o("--creature-distant-turn"),
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
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("Comparacion de la mirada: G alterna NAVE/CAMARA"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "Giro global (candidata): arranque LEJOS 8 radios a 25 grados | max 20/12 grados | t95 8 s | peso 0 hasta 3 radios, 1 desde 7 | H/7/8/R"
        ),
        "{stdout}"
    );
    // El mismo cuadro, armado aqui con la API publica: arranque LEJOS (aunque se pida
    // `--near-creature`), giro en reposo, ojo a la camara y los dos paneles.
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
    world.enable_creature_distant_turn().unwrap();
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
    world.render(&mut fb);
    let lines = eye_comparison_lines(&world.creature_eye_comparison_status().unwrap());
    draw_eye_comparison_panel(&mut fb, &lines);
    let left = eye_comparison_panel(640, 360, &lines);
    let turn_lines = distant_turn_lines(
        &world.creature_distant_turn_status().unwrap(),
        CreaturePreset::Far,
    );
    assert_eq!(turn_lines[0], "GIRO GLOBAL: ON   H APAGA   T95 8 S");
    draw_distant_turn_panel(&mut fb, &turn_lines, &left);
    let (w, h, pixels) = decode(&std::fs::read(&output).unwrap());
    assert_eq!((w, h), (640, 360));
    assert!(
        pixels == fb.pixels,
        "el PNG headless es el cuadro de la candidata"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- D9: replay pilotado y asset real ---

/// Vuelo pilotado `ShipInput -> World::update -> giro -> ojo -> render`: espera lejos, se
/// acerca hasta menos de 3 radios, espera y retrocede. Devuelve por cuadro la distancia de la
/// camara en radios, el giro y la mirada.
fn fly(world: &mut World) -> Vec<(f64, TurnAngles, GazeAngles)> {
    let dt = 1.0 / 60.0;
    let radius = world.creature().unwrap().placement().world_radius();
    let center = world.creature().unwrap().placement().center();
    let distance = |w: &World| (w.camera.position - center).length() / radius;
    let mut trace = Vec::new();
    let mut phase = 0;
    let mut frames_in_phase = 0;
    for frame in 0..2400 {
        let d = distance(world);
        let input = match phase {
            // Lejos, quieta: el giro avanza.
            0 => ShipInput::default(),
            // Acercamiento sin impulso; en punto muerto cuando la frenada (v / 1.5, la
            // respuesta de velocidad de la nave) la deja a unos 2.75 radios.
            1 => ShipInput {
                thrust: if d - world.ship.speed / 1.5 / radius > 2.75 {
                    1.0
                } else {
                    0.0
                },
                ..ShipInput::default()
            },
            // Cerca: espera (el giro vuelve al reposo, el ojo compensa).
            2 => ShipInput::default(),
            // Retroceso lento.
            _ => ShipInput {
                thrust: -1.0,
                ..ShipInput::default()
            },
        };
        world.update(&input, dt);
        world.update_creature_eye_follow(dt);
        if frame % 20 == 0 {
            let (pixels, stats) = render(world);
            assert!(stats.objects_submitted >= 2, "{frame}");
            assert!(pixels.iter().any(|&p| p != SPACE_COLOR), "{frame}");
        }
        trace.push((
            distance(world),
            world.creature_distant_turn().unwrap().current(),
            world.creature_eye_follow().unwrap().current(),
        ));
        frames_in_phase += 1;
        let next = match phase {
            0 => frames_in_phase >= 300,
            1 => d < 2.9 && world.ship.speed.abs() < 1.0,
            2 => frames_in_phase >= 240,
            _ => false,
        };
        if next {
            phase += 1;
            frames_in_phase = 0;
        }
    }
    assert_eq!(phase, 3, "el vuelo llego a retroceder");
    trace
}

#[test]
fn a_flown_far_to_near_and_back_replay_never_jumps_and_the_eye_always_compensates() {
    let mut world = far_world();
    world.debug_oxygen01 = 0.05;
    world.snap_creature_reveal();
    let center = turned_placement().center();
    let trace = fly(&mut world);
    let dt = 1.0 / 60.0;
    let turn_step =
        (1.0 - (-(20f64.ln()) / DISTANT_TURN_T95_SECONDS * dt).exp()) * 2.0 * DISTANT_TURN_MAX_YAW;
    let eye_step = (1.0 - (-(20f64.ln()) / 1.5 * dt).exp()) * 2.0 * 10.0 * DEG;
    let (mut min_d, mut max_turn) = (f64::MAX, 0.0f64);
    for pair in trace.windows(2) {
        let ((_, t0, e0), (d1, t1, e1)) = (pair[0], pair[1]);
        assert!((t1.yaw() - t0.yaw()).hypot(t1.pitch() - t0.pitch()) <= turn_step + 1e-12);
        assert!((e1.yaw() - e0.yaw()).hypot(e1.pitch() - e0.pitch()) <= eye_step + 1e-12);
        min_d = min_d.min(d1);
        max_turn = max_turn.max(t1.yaw().hypot(t1.pitch()));
    }
    assert!(min_d < 3.0 && min_d > 1.2, "{min_d}");
    assert!(trace.last().unwrap().0 > 5.0, "el retroceso vuelve a pesar");
    assert!(max_turn > 10.0 * DEG, "lejos giro de verdad: {max_turn}");
    // Cerca (peso 0) el giro solo decrece: retorno suave, nunca crece ni salta.
    for pair in trace.windows(2).filter(|p| p[0].0 < 3.0 && p[1].0 < 3.0) {
        let (a, b) = (pair[0].1, pair[1].1);
        assert!(b.yaw().hypot(b.pitch()) <= a.yaw().hypot(a.pitch()) + 1e-15);
    }
    // Al final el ojo sigue compensado contra el giro actual.
    assert_eq!(
        world.creature_eye_goal(),
        Some(compensated_goal(&world, world.camera.position))
    );
    assert!((world.ship.position - center).length() > 0.0);
}

#[test]
fn switching_presets_back_and_forth_resets_the_turn_and_never_produces_bad_numbers() {
    let mut world = without_catalogs(turn_world());
    world.debug_oxygen01 = 0.05;
    for (i, preset) in [
        CreaturePreset::Far,
        CreaturePreset::Near,
        CreaturePreset::Far,
        CreaturePreset::Far,
        CreaturePreset::Near,
        CreaturePreset::Near,
    ]
    .into_iter()
    .enumerate()
    {
        world.place_creature_preset(preset).unwrap();
        assert_eq!(
            world.creature_distant_turn().unwrap().current(),
            TurnAngles::NEUTRAL,
            "{i}"
        );
        assert_eq!(
            world.creature_eye_follow().unwrap().current(),
            compensated_goal(&world, world.camera.position),
            "{i}"
        );
        for _ in 0..30 {
            world.update(&ShipInput::default(), 1.0 / 60.0);
            world.update_creature_eye_follow(1.0 / 60.0 + i as f64);
        }
        let turn = world.creature_distant_turn().unwrap().current();
        assert!(turn.yaw().is_finite() && turn.pitch().is_finite());
        assert_eq!(turn.is_neutral(), preset == CreaturePreset::Near, "{i}");
        let (_, stats) = render(&world);
        assert_eq!(stats.objects_submitted, world.system.bodies.len() + 2);
    }
}

/// Asset real de Charlie, solo lectura, con rutas por entorno (nunca en el codigo):
/// `UMBRAL_REAL_CREATURE_OBJ` y `UMBRAL_REAL_CREATURE_TEXTURE`. Se ejecuta a mano con
/// `--ignored`; sin las variables falla, no pasa en vacio.
#[test]
#[ignore = "asset real privado: rutas por variables de entorno"]
fn the_real_asset_turns_rigidly_far_away_and_keeps_its_arrays() {
    use umbral::assets::obj::load_model;
    let obj = PathBuf::from(std::env::var_os("UMBRAL_REAL_CREATURE_OBJ").expect("ruta del OBJ"));
    let png = PathBuf::from(
        std::env::var_os("UMBRAL_REAL_CREATURE_TEXTURE").expect("ruta de la textura"),
    );
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
    world.enable_creature_distant_turn().unwrap();
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    let neutral = {
        let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
        world.render(&mut fb);
        fb.pixels
    };
    let trace = fly(&mut world);
    let max_turn = trace
        .iter()
        .map(|(_, t, _)| t.yaw().hypot(t.pitch()))
        .fold(0.0, f64::max);
    println!(
        "REAL_ASSET frames {} giro maximo {:.3} grados distancia minima {:.3} radios",
        trace.len(),
        max_turn.to_degrees(),
        trace.iter().map(|(d, _, _)| *d).fold(f64::MAX, f64::min)
    );
    assert!(max_turn > 10.0 * DEG);
    // Giro rigido: el centro envolvente del OBJ sigue en el centro de la colocacion.
    let creature = world.creature().unwrap();
    let turn = TurnAngles::clamped(17.0 * DEG, 9.0 * DEG);
    let center = Vec3::from_array(creature.model().bounds_center);
    let moved = turned_transform(creature, turn)
        .matrix()
        .transform_point(center);
    assert!(vec_close(moved, creature.placement().center(), 1e-9));
    // Lejos de nuevo: el giro avanza y se ve.
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    world.update_creature_eye_follow(8.0);
    let turned = {
        let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
        world.render(&mut fb);
        fb.pixels
    };
    assert!(turned != neutral);
    assert_eq!(
        world.creature_eye_goal(),
        Some(compensated_goal(&world, world.camera.position))
    );
    // Arreglos del modelo (vertices, UV, normales, caras) intactos tras el vuelo.
    let fresh = load_model(&obj).unwrap();
    let model = world.creature().unwrap().model();
    assert!(model.vertices == fresh.vertices);
    assert!(model.uvs == fresh.uvs);
    assert!(model.normals == fresh.normals);
    assert_eq!(model.faces.len(), fresh.faces.len());
    assert!(
        model
            .faces
            .iter()
            .zip(&fresh.faces)
            .all(|(a, b)| a.vertex_indices == b.vertex_indices
                && a.uv_indices == b.uv_indices
                && a.normal_indices == b.normal_indices)
    );
    assert_eq!(model.bounds_center, fresh.bounds_center);
}

// --- CORRECTION03: reinicio completo (Shift+R) y estado terminal en el despacho REAL del bucle ---

mod correction03_dispatch {
    use super::*;
    use umbral::{
        app::{DispatchRoute, FrameKeys, dispatch_frame_keys},
        scene::ship::ShipInput,
        visual::perception_game::OxygenKeys,
    };

    fn advance(world: &mut World, frames: usize) {
        for _ in 0..frames {
            world.update(&ShipInput::default(), 1.0 / 60.0);
            world.update_creature_eye_follow(1.0 / 60.0);
        }
    }
    fn route() -> DispatchRoute {
        DispatchRoute {
            distant_turn: true,
            ..DispatchRoute::default()
        }
    }
    fn every_key_but_restart() -> FrameKeys {
        FrameKeys {
            full_restart: false,
            reset: true,
            frontal: true,
            oblique: true,
            follow: true,
            target: true,
            orbit_assist: true,
            real_oxygen: true,
            oxygen: OxygenKeys {
                preset: Some(1.0),
                increase: true,
                decrease: false,
            },
            capture_toggle: true,
            emergency: true,
        }
    }
    fn observable(world: &World) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            world.ship,
            world.camera,
            world.creature_distant_turn(),
            world.creature_eye_follow().map(|f| f.current()),
            world.creature_eye_target(),
            (
                world.debug_oxygen01,
                world.forced_oxygen,
                world.orbit_assist
            ),
            world.time
        )
    }
    /// Mundo de la ruta del giro ya jugado: preset CERCA por 7, giro avanzado y H apagado.
    fn played() -> (World, CreaturePreset) {
        let mut world = turn_world();
        world.debug_oxygen01 = 0.05;
        let mut last = CreaturePreset::Far;
        world.place_creature_preset(CreaturePreset::Far).unwrap();
        advance(&mut world, 90);
        dispatch_frame_keys(
            &mut world,
            None,
            None,
            route(),
            &mut last,
            FrameKeys {
                follow: true,
                ..FrameKeys::default()
            },
        )
        .unwrap();
        advance(&mut world, 30);
        (world, last)
    }

    /// Cubre el `turn` de `World::restart_game` que ninguna prueba afirmaba (mutante m5b de correction02).
    #[test]
    fn correction03_full_restart_restores_the_turn_and_clock_with_preconditions() {
        let (mut world, _) = played();
        let turn = world.creature_distant_turn().unwrap();
        assert!(
            !turn.is_active() && turn.current() != TurnAngles::NEUTRAL,
            "precondicion: giro avanzado y H apagado {turn:?}"
        );
        assert_ne!(turn, DistantTurn::new(), "precondicion");
        assert!(world.time > 0.0, "precondicion: reloj avanzado");
        world.restart_game();
        assert_eq!(world.creature_distant_turn(), Some(DistantTurn::new()));
        assert_eq!(world.time, 0.0);
    }

    #[test]
    fn correction03_terminal_ignores_every_relocation_and_effect_key_but_shift_r() {
        for objective in [false, true] {
            let (mut world, mut last) = played();
            dispatch_frame_keys(
                &mut world,
                None,
                None,
                route(),
                &mut last,
                FrameKeys {
                    frontal: true,
                    ..FrameKeys::default()
                },
            )
            .unwrap();
            assert_eq!(
                last,
                CreaturePreset::Near,
                "precondicion: 7 aplica en juego activo"
            );
            advance(&mut world, 20);
            if objective {
                world.gameplay.objective_complete = true;
            } else {
                world.gameplay.game_over = true;
            }
            let before = observable(&world);
            dispatch_frame_keys(
                &mut world,
                None,
                None,
                route(),
                &mut last,
                every_key_but_restart(),
            )
            .unwrap();
            assert_eq!(
                observable(&world),
                before,
                "terminal aplico R/7/8/H/G/O/0/O2 (objective={objective})"
            );
            assert_eq!(last, CreaturePreset::Near, "terminal cambio el preset");
            dispatch_frame_keys(
                &mut world,
                None,
                None,
                route(),
                &mut last,
                FrameKeys {
                    full_restart: true,
                    ..FrameKeys::default()
                },
            )
            .unwrap();
            assert!(
                !world.gameplay.game_over && !world.gameplay.objective_complete,
                "Shift+R no reinicio desde terminal"
            );
        }
    }

    #[test]
    fn correction03_full_restart_restores_the_route_start_preset_and_turn() {
        let (mut world, mut last) = played();
        dispatch_frame_keys(
            &mut world,
            None,
            None,
            route(),
            &mut last,
            FrameKeys {
                frontal: true,
                ..FrameKeys::default()
            },
        )
        .unwrap();
        advance(&mut world, 30);
        assert_eq!(last, CreaturePreset::Near, "precondicion");
        dispatch_frame_keys(
            &mut world,
            None,
            None,
            route(),
            &mut last,
            FrameKeys {
                full_restart: true,
                ..FrameKeys::default()
            },
        )
        .unwrap();
        // Arranque de la ruta: el preset LEJOS (como `run`), el giro en reposo y el reloj en 0.
        let mut start = turn_world();
        start.place_creature_preset(CreaturePreset::Far).unwrap();
        assert_eq!(
            last,
            CreaturePreset::Far,
            "Shift+R conservo el preset jugado"
        );
        assert_eq!(format!("{:?}", world.ship), format!("{:?}", start.ship));
        assert_eq!(format!("{:?}", world.camera), format!("{:?}", start.camera));
        assert_eq!(world.creature_distant_turn(), Some(DistantTurn::new()));
        assert_eq!(world.time, 0.0);
    }
}

/// CORRECTION03: componente visual puro con los catalogos de gameplay EXPLICITAMENTE vacios (sin las seis
/// balizas integradas en `World::new`); la escena integrada con balizas se prueba aparte.
fn without_catalogs(mut world: World) -> World {
    world.gameplay.resources.clear();
    world.gameplay.logs.clear();
    world
}

#[test]
fn correction03_the_integrated_turn_world_draws_its_six_beacons_too() {
    let mut world = turn_world();
    world.debug_oxygen01 = 0.05;
    world.place_creature_preset(CreaturePreset::Far).unwrap();
    let beacons = world.gameplay.resources.len() + world.gameplay.logs.len();
    assert_eq!(beacons, 6);
    let (_, stats) = render(&world);
    assert_eq!(
        stats.objects_submitted,
        world.system.bodies.len() + 2 + beacons
    );
}
