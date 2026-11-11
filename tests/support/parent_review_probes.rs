// Parent-only regressions. Existing 1177-line test body stays an exact byte prefix.
use super::*;

fn parent_review_snapshot(g: &Game, carry: &CaptureCarry) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{:?}",
        g.world.ship,
        (
            g.world.camera.position,
            g.world.camera.target,
            g.world.camera.up
        ),
        g.oxygen,
        carry.status(),
        g.game.title(&g.world),
        g.game.view(),
        (g.world.time.to_bits(), g.world.debug_oxygen01.to_bits()),
        carry.body_override().map(|o| o.extra_bounds_obj),
    )
}

fn parent_review_released_identity(kind: &str) {
    use umbral::renderer::texture::Texture;
    fixture!(g, carry, true);
    to_lock(&mut g, &mut carry);
    g.world.debug_oxygen01 = 0.25;
    ok(
        carry.frame(&mut g.world, DT, PreviewKeys::default()),
        "actual oxygen release",
    );
    assert_eq!(carry.status().phase, CarryPhase::Released);
    assert!(!carry.locks_flight());
    ok(
        carry.frame(&mut g.world, 0.0, PreviewKeys::default()),
        "valid released matching control",
    );
    let placement = g.world.creature().unwrap().placement();
    let mut foreign = load_model(&env("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
    let dimensions = (
        foreign.vertices.len(),
        foreign.normals.len(),
        foreign.uvs.len(),
        foreign.faces.len(),
    );
    match kind {
        "materials" => foreign
            .material_libraries
            .push("parent_identity_metadata_only.mtl".into()),
        "faces" => foreign.faces.swap(0, 1),
        _ => unreachable!(),
    }
    assert_eq!(
        dimensions,
        (
            foreign.vertices.len(),
            foreign.normals.len(),
            foreign.uvs.len(),
            foreign.faces.len()
        )
    );
    let texture = Texture::load_png(&env("UMBRAL_CREATURE_TEXTURE")).unwrap();
    let creature = StaticCreature::new(foreign, texture, placement).expect("valid foreign model");
    ok(
        g.world.set_creature(creature),
        "public valid scene replacement",
    );
    let before = parent_review_snapshot(&g, &carry);
    let before_arm = carry.arm().map(|(v, n)| (v.to_vec(), n.to_vec()));
    for keys in [
        PreviewKeys::default(),
        PreviewKeys {
            request: true,
            reset: false,
        },
        PreviewKeys {
            request: false,
            reset: true,
        },
        PreviewKeys {
            request: true,
            reset: true,
        },
    ] {
        let result = carry.frame(&mut g.world, DT, keys);
        println!("PARENT_RELEASED {kind} rejected={}", result.is_err());
        assert!(
            result.is_err(),
            "CORE-02: Released accepted foreign {kind} identity with matching geometry prefixes"
        );
        assert_eq!(
            parent_review_snapshot(&g, &carry),
            before,
            "released state atomic"
        );
        assert!(carry.arm().map(|(v, n)| (v.to_vec(), n.to_vec())) == before_arm);
    }
    carry.abort(
        &mut g.world,
        "parent explicit abort after identity rejection",
    );
    assert!(!carry.locks_flight() && carry.body_override().is_none());
}

#[test]
#[ignore = "assets privados; parent Released identity regression"]
fn parent_review_released_materials() {
    parent_review_released_identity("materials");
}
#[test]
#[ignore = "assets privados; parent Released identity regression"]
fn parent_review_released_faces() {
    parent_review_released_identity("faces");
}

fn parent_review_shared_invalid(kind: &str) {
    fixture!(g, carry, true);
    ok(
        g.game.apply(
            &mut g.world,
            SolarKeys {
                oblique: true,
                ..SolarKeys::default()
            },
        ),
        "valid oblique control",
    );
    let mut controls = neutral();
    let elapsed = if kind == "negative_elapsed" { -1.0 } else { DT };
    if kind == "negative_elapsed" {
        controls.solar.reset = true;
    } else {
        controls.solar.follow = true;
    }
    match kind {
        "negative_elapsed" => {}
        "nan_oxygen" => controls.oxygen.preset = Some(f64::NAN),
        "nan_thrust" => controls.ship.thrust = f64::NAN,
        "inf_yaw" => controls.ship.yaw = f64::INFINITY,
        "nan_pitch" => controls.ship.pitch = f64::NAN,
        "inf_roll" => controls.ship.roll = f64::INFINITY,
        _ => unreachable!(),
    }
    let before = parent_review_snapshot(&g, &carry);
    let result = advance_capture_frame(
        &mut g.world,
        &mut g.game,
        &mut g.oxygen,
        &mut carry,
        controls,
        elapsed,
    );
    let unchanged = parent_review_snapshot(&g, &carry) == before;
    println!(
        "PARENT_SHARED {kind} rejected={} unchanged={unchanged}",
        result.is_err()
    );
    assert!(
        result.is_err() && unchanged,
        "RUNTIME-01: shared entry accepted invalid {kind} or mutated before rejection"
    );
}

#[test]
#[ignore = "assets privados; parent shared-entry rejection"]
fn parent_review_shared_negative_elapsed() {
    parent_review_shared_invalid("negative_elapsed");
}
#[test]
#[ignore = "assets privados; parent shared-entry rejection"]
fn parent_review_shared_nan_oxygen() {
    parent_review_shared_invalid("nan_oxygen");
}
#[test]
#[ignore = "assets privados; parent shared-entry rejection"]
fn parent_review_shared_nan_thrust() {
    parent_review_shared_invalid("nan_thrust");
}
#[test]
#[ignore = "assets privados; parent shared-entry rejection"]
fn parent_review_shared_inf_yaw() {
    parent_review_shared_invalid("inf_yaw");
}
#[test]
#[ignore = "assets privados; parent shared-entry rejection"]
fn parent_review_shared_nan_pitch() {
    parent_review_shared_invalid("nan_pitch");
}
#[test]
#[ignore = "assets privados; parent shared-entry rejection"]
fn parent_review_shared_inf_roll() {
    parent_review_shared_invalid("inf_roll");
}
