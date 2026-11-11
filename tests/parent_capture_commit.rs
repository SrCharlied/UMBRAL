use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    scene::world::World,
};

#[test]
fn parent_commit_rejects_finite_position_with_collapsed_camera_atomically() {
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 1280.0 / 720.0).unwrap();
    let initial_position = world.ship.position;
    let initial_orientation = world.ship.orientation;
    world
        .commit_captured_ship_pose(initial_position, initial_orientation)
        .expect("valid matching pose control");
    assert!((world.camera.target - world.camera.position).length() > 0.0);
    let before_ship = format!("{:?}", world.ship);
    let before_camera = (world.camera.position, world.camera.target, world.camera.up);
    let huge_but_finite = Vec3::new(1e200, 1e200, 1e200);
    assert!(huge_but_finite.is_finite());
    let result = world.commit_captured_ship_pose(huge_but_finite, initial_orientation);
    println!(
        "PARENT_COMMIT rejected={} camera_collapsed={} ship_changed={}",
        result.is_err(),
        world.camera.position == world.camera.target,
        format!("{:?}", world.ship) != before_ship,
    );
    assert!(
        result.is_err(),
        "CAPTURE-CAMERA: finite capture position accepted but chase camera collapsed; must reject before publishing ship/camera"
    );
    assert_eq!(format!("{:?}", world.ship), before_ship);
    assert_eq!(
        (world.camera.position, world.camera.target, world.camera.up),
        before_camera,
    );
}

#[test]
fn fix01_camera_mathematical_domain_preserves_full_state_on_rejection() {
    use umbral::{math::Mat4, scene::ship::ShipInput};
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    w.update(
        &ShipInput {
            thrust: 0.8,
            yaw: 0.4,
            pitch: -0.3,
            roll: 0.2,
            boost: true,
        },
        0.01,
    );
    let orientation = w.ship.orientation;
    for position in [Vec3::new(1e20, 1e20, 1e20), Vec3::new(1e200, 1e200, 1e200)] {
        let before = format!("{:?}|{:?}", w.ship, w.camera);
        assert!(w.commit_captured_ship_pose(position, orientation).is_err());
        assert_eq!(format!("{:?}|{:?}", w.ship, w.camera), before);
    }
    // Large coordinate alone is NOT invalid: relative chase offsets remain representable.
    w.commit_captured_ship_pose(Vec3::new(1e12, 1e12, 1e12), Mat4::IDENTITY)
        .unwrap();
    assert!((w.camera.target - w.camera.position).length() > 0.0);
    for (distance, height) in [(-30.0, 0.0), (f64::INFINITY, 6.0), (f64::MAX, f64::MAX)] {
        w.chase_distance = distance;
        w.chase_height = height;
        let before = format!("{:?}|{:?}", w.ship, w.camera);
        assert!(
            w.commit_captured_ship_pose(Vec3::ZERO, Mat4::IDENTITY)
                .is_err()
        );
        assert_eq!(format!("{:?}|{:?}", w.ship, w.camera), before);
    }
}
