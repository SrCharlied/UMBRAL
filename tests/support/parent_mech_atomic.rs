//! Parent regression for MECH-ATOMIC-01. Uses unchanged real fixture helpers from enclosing test.
use super::*;
fn snapshot(g: &Game, auto: &AutoCapture) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}",
        g.world.ship,
        g.world.camera,
        auto.status(),
        g.world.time.to_bits(),
        g.world.debug_oxygen01.to_bits()
    )
}
fn check(foreign: bool) {
    fixture!(g, auto);
    let (_, out) = fly_to_contact(&mut g, &mut auto, coast_spin());
    assert!(has(&st(&out), &AutoEvent::Contact) && auto.holds_body() && auto.flight_locked());
    ok(
        auto.frame(&mut g.world, 0.0, false),
        "valid direct closing control",
    );
    if foreign {
        use umbral::renderer::texture::Texture;
        let placement = g.world.creature().unwrap().placement();
        let mut model = load_model(&env("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
        model
            .material_libraries
            .push("parent_identity_metadata_only.mtl".into());
        let texture = Texture::load_png(&env("UMBRAL_CREATURE_TEXTURE")).unwrap();
        let creature =
            StaticCreature::new(model, texture, placement).expect("valid foreign scene identity");
        ok(
            g.world.set_creature(creature),
            "legal public scene replacement",
        );
    }
    g.world.camera.position.x += 1.0;
    let before = snapshot(&g, &auto);
    let result = auto.frame(&mut g.world, if foreign { DT } else { f64::NAN }, false);
    let unchanged = snapshot(&g, &auto) == before;
    println!(
        "PARENT_MECH_ATOMIC foreign={foreign} rejected={} world_status_unchanged={unchanged}",
        result.is_err()
    );
    assert!(
        result.is_err() && unchanged,
        "MECH-ATOMIC-01: direct frame rejected after mutating World"
    );
}
#[test]
#[ignore = "private assets, direct atomicity regression"]
fn parent_mech_invalid_dt_preserves_ship_camera_and_status() {
    check(false);
}
#[test]
#[ignore = "private assets, direct atomicity regression"]
fn parent_mech_foreign_scene_preserves_ship_camera_and_status() {
    check(true);
}
