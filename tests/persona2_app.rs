use umbral::{
    app::{GameplayAction, apply_gameplay_action, gameplay_feedback_lines},
    assets::{obj::load_model, ship_model_path},
    scene::world::World,
};

fn world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}

#[test]
fn game_controls_toggle_orbit_and_restart_the_whole_gameplay_state() {
    let mut w = world();
    apply_gameplay_action(&mut w, GameplayAction::ToggleOrbitAssist);
    assert!(w.orbit_assist);
    apply_gameplay_action(&mut w, GameplayAction::ForceOxygen(0.05));
    assert_eq!(w.oxygen01(), 0.05);
    assert_eq!(w.gameplay.oxygen01, 1.0, "debug must not falsify gameplay");
    apply_gameplay_action(&mut w, GameplayAction::RealOxygen);
    assert_eq!(w.oxygen01(), 1.0);
    w.gameplay.game_over = true;
    w.gameplay.collected_logs = 2;
    apply_gameplay_action(&mut w, GameplayAction::Restart);
    assert!(!w.gameplay.game_over && !w.orbit_assist);
    assert_eq!(w.gameplay.collected_logs, 0);
    assert!(w.gameplay.logs.iter().all(|log| !log.anchor.collected));
}

#[test]
fn orbital_camera_keeps_the_assisted_planet_visible_even_from_the_pole() {
    for radial in [umbral::math::Vec3::X, umbral::math::Vec3::Y] {
        let mut w = world();
        let center = w.system.bodies[1].position(w.time);
        w.ship = umbral::scene::ship::Ship::new(center + radial * 40.0, -umbral::math::Vec3::Z);
        w.orbit_assist = true;
        w.update(&umbral::scene::ship::ShipInput::default(), 1.0 / 60.0);
        let index = w
            .gameplay
            .navigation
            .assisted_planet
            .expect("assist should engage");
        let to_planet = (w.system.bodies[index].position(w.time) - w.camera.position).normalized();
        assert!(
            w.camera.forward().dot(to_planet) > 0.999,
            "camera loses planet"
        );
        assert!(
            w.camera.up.cross(to_planet).length() > 0.1,
            "camera up degenerates near pole"
        );
    }
}

#[test]
fn polar_orbit_camera_uses_the_stable_orbital_up_axis() {
    let mut w = world();
    let center = w.system.bodies[1].position(w.time);
    w.ship = umbral::scene::ship::Ship::new(
        center + umbral::math::Vec3::Y * 40.0,
        -umbral::math::Vec3::Z,
    );
    w.orbit_assist = true;
    for _ in 0..1000 {
        w.update(&umbral::scene::ship::ShipInput::default(), 1.0 / 60.0);
        assert!(
            w.camera.up.dot(w.ship.up()) > 0.999,
            "camera up jumps at polar threshold"
        );
        let distance = (w.camera.position - w.ship.position).length();
        assert!((distance - 22.0_f64.hypot(6.0)).abs() < 1e-8);
    }
}

#[test]
fn feedback_guides_the_player_toward_uncollected_logs() {
    let mut w = world();
    assert!(gameplay_feedback_lines(&w).join(" ").contains("DESTINO"));
    for log in &mut w.gameplay.logs {
        log.anchor.collected = true;
    }
    w.gameplay.collected_logs = 3;
    assert!(gameplay_feedback_lines(&w).join(" ").contains("SOL"));
}

#[test]
fn feedback_exposes_real_oxygen_counts_objective_and_terminal_state() {
    let mut w = world();
    w.gameplay.oxygen01 = 0.37;
    w.gameplay.collected_resources = 2;
    w.gameplay.collected_logs = 1;
    let lines = gameplay_feedback_lines(&w).join(" ");
    assert!(
        lines.contains("37%") && lines.contains("RECURSOS 2") && lines.contains("REGISTROS 1/3")
    );
    w.gameplay.game_over = true;
    assert!(
        gameplay_feedback_lines(&w)
            .join(" ")
            .contains("R: REINICIAR")
    );
    w.gameplay.game_over = false;
    w.gameplay.objective_complete = true;
    assert!(
        gameplay_feedback_lines(&w)
            .join(" ")
            .contains("OBJETIVO COMPLETO")
    );
}
