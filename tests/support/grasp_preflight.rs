//! Preparation only: real contact calibration, approved closure data, and first RED witness.
//! No production implementation and no World/camera/ship mutation.
use super::*;

fn centroid(points: &[[f64; 3]], indices: &[usize]) -> Vec3 {
    assert!(!indices.is_empty());
    let mut sum = Vec3::ZERO;
    for &i in indices {
        assert!(i < points.len() && points[i].iter().all(|v| v.is_finite()));
        sum = sum + v3(points[i]);
    }
    sum * (1.0 / indices.len() as f64)
}
fn world_tip_obj(world: &World) -> Vec3 {
    let c = world.creature_render_transform().unwrap().matrix();
    let rel = world.ship.position - c.column(3);
    let scale2 = c.column(0).dot(c.column(0));
    assert!(scale2.is_finite() && scale2 > 0.0);
    Vec3::new(
        rel.dot(c.column(0)) / scale2,
        rel.dot(c.column(1)) / scale2,
        rel.dot(c.column(2)) / scale2,
    )
}

#[test]
#[ignore = "private real model/package paths supplied by run_preflight.py"]
fn preparation01_measure_anatomical_contact_without_modifying_geometry() {
    let (world, _solar, pursuit) = real_game(0.05);
    let rig = pursuit.rig().unwrap();
    let data = pursuit.debug().package().data();
    let topo = data.topology();
    assert_eq!(topo.root().len(), 27);
    assert_eq!(topo.grip().len(), 105);
    let root = v3(rig.rest_loop().center);
    let tangent = v3(rig.rest_loop().tangent);
    let side = tangent.cross(Vec3::Y).normalized();
    let up = side.cross(tangent).normalized();
    let rest_contact = centroid(rig.rest_vertices(), topo.grip());
    let rest_lateral = rest_contact - v3(rig.rest_tip());
    let transform = world.creature_render_transform().unwrap().matrix();
    for (label, target) in [
        ("actual_ship", world_tip_obj(&world)),
        ("straight", root + tangent * 3.0),
        (
            "oblique",
            root + (-tangent * 0.4 + side * 0.8 + up * 0.2).normalized() * 3.0,
        ),
    ] {
        let mut vertices = data.template().vertices.clone();
        let mut normals = data.template().normals.clone();
        let pose = rig
            .deform(
                [target.x, target.y, target.z],
                None,
                &mut vertices,
                &mut normals,
            )
            .unwrap();
        assert!(
            vertices
                .iter()
                .chain(&normals)
                .all(|a| a.iter().all(|x| x.is_finite()))
        );
        let contact = centroid(&vertices, topo.grip());
        let offset = contact - target;
        let rotated = Vec3::new(
            pose.end_rotation[0][0] * rest_lateral.x
                + pose.end_rotation[0][1] * rest_lateral.y
                + pose.end_rotation[0][2] * rest_lateral.z,
            pose.end_rotation[1][0] * rest_lateral.x
                + pose.end_rotation[1][1] * rest_lateral.y
                + pose.end_rotation[1][2] * rest_lateral.z,
            pose.end_rotation[2][0] * rest_lateral.x
                + pose.end_rotation[2][1] * rest_lateral.y
                + pose.end_rotation[2][2] * rest_lateral.z,
        );
        assert!((v3(pose.axial_tip) - target).length() < 1e-9);
        assert!(
            (offset - rotated).length() < 1e-9,
            "contact offset follows distal rotation, not a constant screen shift"
        );
        for &i in topo.root() {
            assert!((v3(vertices[i]) - v3(rig.rest_vertices()[i])).length() < 1e-12);
        }
        let world_offset = transform.transform_dir(offset);
        println!(
            "GRASP_PREFLIGHT {{\"kind\":\"contact_offset\",\"case\":\"{label}\",\"offset_obj\":{:?},\"offset_world\":{:?},\"distance_obj\":{},\"distance_world\":{}}}",
            [offset.x, offset.y, offset.z],
            [world_offset.x, world_offset.y, world_offset.z],
            offset.length(),
            world_offset.length()
        );
    }
}

#[test]
#[ignore = "private real model/package paths supplied by run_preflight.py"]
fn preparation02_reuse_approved_closure_data_without_applying_ship_or_camera_track() {
    let (world, _solar, pursuit) = real_game(0.05);
    let package = pursuit.debug().package();
    let data = package.data();
    let capture = package.capture();
    let ship_before = world.ship.position;
    let camera_before = world.camera.view_projection();
    assert_eq!(capture.frames(), 312);
    assert_eq!(capture.fps(), 24);
    assert_eq!(capture.manifest().grip_frame(), 168);
    assert_eq!(data.frames(), capture.frames());
    println!(
        "GRASP_PREFLIGHT {{\"kind\":\"clip\",\"frames\":{},\"fps\":{},\"grip_frame\":{},\"duration_seconds\":{}}}",
        capture.frames(),
        capture.fps(),
        capture.manifest().grip_frame(),
        capture.duration_s()
    );
    for (frame, stage) in capture.manifest().stages() {
        println!("GRASP_STAGE {frame} {stage}");
    }
    let mut output = copy_model(data.template());
    let mut rest = copy_model(data.template());
    data.evaluate(1, [0.0; 3], &mut rest).unwrap();
    for frame in [1, 48, 100, 132, 150, 168, 312] {
        data.evaluate(frame, [0.0; 3], &mut output).unwrap();
        assert!(
            output
                .vertices
                .iter()
                .chain(&output.normals)
                .all(|p| p.iter().all(|v| v.is_finite()))
        );
        let mut root_error = 0.0_f64;
        for &i in data.topology().root() {
            let error = (v3(output.vertices[i]) - v3(rest.vertices[i])).length();
            assert!(error.is_finite());
            root_error = root_error.max(error);
        }
        assert!(
            root_error < 1e-9,
            "approved clip root changed at frame{frame}: {root_error}"
        );
        let g = centroid(&output.vertices, data.topology().grip());
        println!(
            "GRASP_PREFLIGHT {{\"kind\":\"clip_sample\",\"frame\":{frame},\"grip_centroid_obj\":{:?},\"root_error_obj\":{root_error}}}",
            [g.x, g.y, g.z]
        );
    }
    assert_eq!(world.ship.position, ship_before);
    assert_eq!(world.camera.view_projection(), camera_before);
}

#[test]
#[ignore = "future anatomical-contact contract: behavioral RED until contact adapter exists"]
fn preparation03_first_contact_alignment_contract_is_not_implemented_yet() {
    let (world, _solar, pursuit) = real_game(0.05);
    let rig = pursuit.rig().unwrap();
    let data = pursuit.debug().package().data();
    let target = world_tip_obj(&world);
    let mut vertices = data.template().vertices.clone();
    let mut normals = data.template().normals.clone();
    let pose = rig
        .deform(
            [target.x, target.y, target.z],
            None,
            &mut vertices,
            &mut normals,
        )
        .unwrap();
    assert!((v3(pose.axial_tip) - target).length() < 1e-9);
    let contact = centroid(&vertices, data.topology().grip());
    let gap = (contact - target).length();
    assert!(gap.is_finite());
    println!(
        "GRASP_PREFLIGHT {{\"kind\":\"first_contact_contract\",\"gap_obj\":{gap},\"tolerance_obj\":0.00001}}"
    );
    assert!(
        gap <= 1e-5,
        "GRASP-CONTACT-01 anatomical contact centroid is not aligned: gap={gap} OBJ; axial target alone is insufficient"
    );
}

// The centroid need not equal the ship origin in the approved closed pose. Preserve that authored
// offset instead of treating the preceding diagnostic as an arbitrary final placement oracle.
fn approved_contact_anchor(
    package: &umbral::visual::tentacle_growth_cpu::GrowthPackage,
) -> (Vec3, Vec3, Vec3) {
    let data = package.data();
    let capture = package.capture();
    let frame = capture.manifest().grip_frame();
    let mut model = copy_model(data.template());
    data.evaluate(frame, [0.0; 3], &mut model).unwrap();
    let contact = centroid(&model.vertices, data.topology().grip());
    let ship = capture.manifest().ship_obj_matrix(frame).unwrap();
    let rel = contact - ship.column(3);
    let scale2 = ship.column(0).dot(ship.column(0));
    assert!(scale2.is_finite() && scale2 > 0.0);
    let anchor_local = Vec3::new(
        rel.dot(ship.column(0)) / scale2,
        rel.dot(ship.column(1)) / scale2,
        rel.dot(ship.column(2)) / scale2,
    );
    assert!((ship.transform_point(anchor_local) - contact).length() < 1e-9);
    (contact, ship.column(3), anchor_local)
}

#[test]
#[ignore = "private real model/package paths supplied by run_preflight.py"]
fn preparation04_calibrate_the_approved_grasp_ship_relative_contact_anchor() {
    let (_world, _solar, pursuit) = real_game(0.05);
    let (contact, ship_origin, anchor) = approved_contact_anchor(pursuit.debug().package());
    let delta = contact - ship_origin;
    println!(
        "GRASP_PREFLIGHT {{\"kind\":\"approved_contact_anchor\",\"grip_frame\":168,\"centroid_minus_ship_origin_obj\":{:?},\"contact_anchor_ship_local\":{:?}}}",
        [delta.x, delta.y, delta.z],
        [anchor.x, anchor.y, anchor.z]
    );
}

#[test]
#[ignore = "future contact adapter contract derived from approved grip pose, not screen-left"]
fn preparation05_first_approved_contact_anchor_contract_is_not_implemented_yet() {
    let (world, _solar, pursuit) = real_game(0.05);
    let rig = pursuit.rig().unwrap();
    let package = pursuit.debug().package();
    let (_, _, anchor) = approved_contact_anchor(package);
    let desired_world = world
        .ship
        .transform(SHIP_MODEL_YAW)
        .matrix()
        .transform_point(anchor);
    let body = world.creature_render_transform().unwrap().matrix();
    let rel = desired_world - body.column(3);
    let scale2 = body.column(0).dot(body.column(0));
    let desired_obj = Vec3::new(
        rel.dot(body.column(0)) / scale2,
        rel.dot(body.column(1)) / scale2,
        rel.dot(body.column(2)) / scale2,
    );
    let target = world_tip_obj(&world);
    let mut vertices = package.data().template().vertices.clone();
    let mut normals = package.data().template().normals.clone();
    rig.deform(
        [target.x, target.y, target.z],
        None,
        &mut vertices,
        &mut normals,
    )
    .unwrap();
    let contact = centroid(&vertices, package.data().topology().grip());
    let gap = (contact - desired_obj).length();
    assert!(gap.is_finite());
    println!(
        "GRASP_PREFLIGHT {{\"kind\":\"approved_contact_contract\",\"gap_obj\":{gap},\"tolerance_obj\":0.00001}}"
    );
    assert!(
        gap <= 1e-5,
        "GRASP-CONTACT-02 current pursuit does not yet align anatomical contact to the approved ship-relative anchor: gap={gap} OBJ"
    );
}
