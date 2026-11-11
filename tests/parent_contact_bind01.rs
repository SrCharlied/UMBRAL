//! Parent regression CONTACT-BIND-01: validated synthetic packages with equal-size but differently
//! indexed distal anatomy. No private asset edits; fixture bytes remain unchanged.
#![allow(dead_code)]
use umbral::{
    assets::{
        obj::{Face, Model, load_model, parse_obj},
        ship_model_path,
    },
    math::{Mat4, Vec3},
    renderer::{framebuffer::Framebuffer, stats::RenderStats, texture::Texture},
    scene::world::{SHIP_MODEL_YAW, SPACE_COLOR, SolarBodyOverride, World},
    visual::{
        solar_game::{SolarGame, solar_creature_placement},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::StaticCreature,
        tentacle_grasp_transition::{CONTACT_TOLERANCE_OBJ, ContactAdapter},
        tentacle_growth_cpu::{
            GrowthPackage,
            capture::{fnv1a64, geometry_fnv},
        },
        tentacle_solar_game::SolarCaptureDebug,
        tentacle_spatial_pursuit::SpatialRig,
        watching_stars_game::render_game_frame_with,
    },
};
include!("support/spatial_pursuit_fixture.rs");

fn permute_growth(g: &str) -> String {
    let mut rows: Vec<String> = g.lines().map(str::to_string).collect();
    let vertex_rows: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.starts_with("vertex ").then_some(i))
        .collect();
    assert_eq!(vertex_rows.len(), 72);
    for k in 0..6 {
        rows.swap(vertex_rows[42 + k], vertex_rows[48 + k]);
    }
    let remap = |x: usize| match x {
        42..=47 => x + 6,
        48..=53 => x - 6,
        _ => x,
    };
    for row in &mut rows {
        let mut t: Vec<String> = row.split_whitespace().map(str::to_string).collect();
        let selected: Vec<usize> = match t[0].as_str() {
            "loop_p" | "loop_d" => (1..t.len()).collect(),
            "ring" => (2..t.len()).collect(),
            "mask" => (3..t.len()).collect(),
            "face" => (2..5).collect(),
            _ => Vec::new(),
        };
        for i in selected {
            t[i] = remap(t[i].parse().unwrap()).to_string();
        }
        *row = t.join(" ");
    }
    rows.join("\n") + "\n"
}
fn p_centroid(v: &[[f64; 3]], ids: &[usize]) -> Vec3 {
    ids.iter()
        .map(|&i| Vec3::from_array(v[i]))
        .fold(Vec3::ZERO, |a, b| a + b)
        * (1.0 / ids.len() as f64)
}
fn p_ship(adapter: &ContactAdapter, rig: &SpatialRig, measured: &[usize], target: Vec3) -> Mat4 {
    let mut v = rig.rest_vertices().to_vec();
    let mut n = rig.rest_normals().to_vec();
    rig.deform([target.x, target.y, target.z], None, &mut v, &mut n)
        .unwrap();
    let d = p_centroid(&v, measured) - target;
    let a = Vec3::from_array(adapter.anchor().ship_local());
    let (an, dn) = (a.normalized(), d.normalized());
    let axis = an.cross(dn);
    assert!(axis.length() > 1e-8 && a.length() > 1e-8 && d.length() > 1e-8);
    let angle = an.dot(dn).clamp(-1.0, 1.0).acos();
    let s = d.length() / a.length();
    let mut m = Mat4::rotation_axis(axis, angle) * Mat4::scaling(Vec3::new(s, s, s));
    m.m[0][3] = target.x;
    m.m[1][3] = target.y;
    m.m[2][3] = target.z;
    assert!((m.transform_dir(a) - d).length() < 1e-12);
    m
}
#[test]
fn parent_contact_bind01_equal_size_wrong_rig_must_reject_atomically() {
    let capture = manifest();
    let growth_a = growth_text(&capture);
    let growth_b = permute_growth(&growth_a);
    let package_a = GrowthPackage::from_parts(
        &capture,
        (1..=FRAMES).map(tube).collect(),
        tube(1),
        &growth_a,
    )
    .unwrap();
    let package_b = GrowthPackage::from_parts(
        &capture,
        (1..=FRAMES).map(tube).collect(),
        tube(1),
        &growth_b,
    )
    .unwrap();
    let rig_a = SpatialRig::new(package_a.data(), &tube(1).vertices).unwrap();
    let rig_b = SpatialRig::new(package_b.data(), &tube(1).vertices).unwrap();
    let adapter_a = ContactAdapter::new(&package_a, &tube(1).vertices).unwrap();
    assert_eq!(rig_a.rest_vertices().len(), rig_b.rest_vertices().len());
    assert_eq!(rig_a.rest_normals().len(), rig_b.rest_normals().len());
    assert_eq!(adapter_a.grip(), &[48, 49, 50, 51, 52, 53]);
    assert_eq!(
        package_b.data().topology().grip(),
        &[42, 43, 44, 45, 46, 47]
    );
    let center = Vec3::from_array(rig_a.rest_loop().center);
    let target = center + Vec3::X * 3.0;
    let good_ship = p_ship(&adapter_a, &rig_a, adapter_a.grip(), target);
    let mut good_v = rig_a.rest_vertices().to_vec();
    let mut good_n = rig_a.rest_normals().to_vec();
    let good = adapter_a
        .solve(
            adapter_a.rig(),
            &good_ship,
            &Mat4::IDENTITY,
            None,
            &mut good_v,
            &mut good_n,
        )
        .unwrap();
    assert!(good.contact_error_obj <= CONTACT_TOLERANCE_OBJ);
    let ship = p_ship(&adapter_a, &rig_b, adapter_a.grip(), target);
    let mut v = vec![[7.0; 3]; rig_b.rest_vertices().len()];
    let mut n = vec![[7.0; 3]; rig_b.rest_normals().len()];
    let before = (v.clone(), n.clone());
    let result = adapter_a.solve(&rig_b, &ship, &Mat4::IDENTITY, None, &mut v, &mut n);
    if let Ok(solution) = result.as_ref() {
        let actual_grip = p_centroid(&v, package_b.data().topology().grip());
        let desired = Vec3::from_array(
            adapter_a
                .anchor()
                .desired_obj(&ship, &Mat4::IDENTITY)
                .unwrap(),
        );
        let gap = (actual_grip - desired).length();
        println!(
            "CONTACT_BIND01 {{\"reported_error_obj\":{},\"true_grip_error_obj\":{gap},\"matching_control_error_obj\":{},\"evaluations\":{}}}",
            solution.contact_error_obj, good.contact_error_obj, solution.evaluations
        );
        assert!(
            gap > CONTACT_TOLERANCE_OBJ,
            "directed witness must miss true grip"
        );
    }
    assert!(
        result.is_err(),
        "CONTACT-BIND-01: independently valid rigB with same sizes published the wrong grip as aligned"
    );
    assert_eq!(
        (v, n),
        before,
        "CONTACT-BIND-01: mismatch must preserve buffers"
    );
}

include!("support/grasp_transition_fixture.rs");

#[test]
fn contact_bind01_owned_a_and_permuted_b_publish_only_their_true_grip() {
    let capture = manifest();
    let a = growth_text(&capture);
    for growth in [a.clone(), permute_growth(&a)] {
        let package =
            GrowthPackage::from_parts(&capture, (1..=FRAMES).map(tube).collect(), tube(1), &growth)
                .unwrap();
        let adapter = ContactAdapter::new(&package, &tube(1).vertices).unwrap();
        let rig = adapter.rig();
        let target = Vec3::from_array(rig.rest_loop().center) + Vec3::X * 3.0;
        let ids = package.data().topology().grip();
        let ship = p_ship(&adapter, rig, ids, target);
        let mut v = vec![[7.0; 3]; rig.rest_vertices().len()];
        let mut n = vec![[7.0; 3]; rig.rest_normals().len()];
        let solution = adapter
            .solve(rig, &ship, &Mat4::IDENTITY, None, &mut v, &mut n)
            .unwrap();
        let desired = Vec3::from_array(
            adapter
                .anchor()
                .desired_obj(&ship, &Mat4::IDENTITY)
                .unwrap(),
        );
        let gap = (p_centroid(&v, ids) - desired).length();
        assert!(gap <= CONTACT_TOLERANCE_OBJ, "true grip gap {gap}");
        assert!((solution.contact_error_obj - gap).abs() < 1e-12);
        for &i in package.data().topology().root() {
            assert_eq!(v[i], rig.rest_vertices()[i]);
        }
        println!("CONTACT_BIND01_OWNED true_grip={ids:?} gap={gap}");
    }
}

#[test]
fn contact_bind01_small_foreign_rig_returns_err_without_panic_or_publication() {
    let capture = manifest();
    let package = GrowthPackage::from_parts(
        &capture,
        (1..=FRAMES).map(tube).collect(),
        tube(1),
        &growth_text(&capture),
    )
    .unwrap();
    let small = GrowthPackage::from_parts(
        &capture,
        (1..=FRAMES).map(tube).collect(),
        tube(1),
        &growth_text_with_rings(&capture, 2),
    )
    .unwrap();
    let adapter = ContactAdapter::new(&package, &tube(1).vertices).unwrap();
    let foreign = SpatialRig::new(small.data(), &tube(1).vertices).unwrap();
    assert!(foreign.rest_vertices().len() < adapter.rig().rest_vertices().len());
    let mut v = vec![[7.0; 3]; foreign.rest_vertices().len()];
    let mut n = vec![[7.0; 3]; foreign.rest_normals().len()];
    let before = (v.clone(), n.clone());
    let result = adapter.solve(
        &foreign,
        &Mat4::IDENTITY,
        &Mat4::IDENTITY,
        None,
        &mut v,
        &mut n,
    );
    assert!(result.unwrap_err().contains("rig ajeno"));
    assert_eq!((v, n), before);
    println!(
        "CONTACT_BIND01_SMALL foreign={} owned={}",
        foreign.rest_vertices().len(),
        adapter.rig().rest_vertices().len()
    );
}

#[test]
fn contact_bind01_independent_identical_rig_is_not_an_identity_capability() {
    let capture = manifest();
    let package = GrowthPackage::from_parts(
        &capture,
        (1..=FRAMES).map(tube).collect(),
        tube(1),
        &growth_text(&capture),
    )
    .unwrap();
    let adapter = ContactAdapter::new(&package, &tube(1).vertices).unwrap();
    let foreign = SpatialRig::new(package.data(), &tube(1).vertices).unwrap();
    assert_eq!(foreign.rest_vertices(), adapter.rig().rest_vertices());
    assert_eq!(foreign.rest_normals(), adapter.rig().rest_normals());
    let mut v = vec![[7.0; 3]; foreign.rest_vertices().len()];
    let mut n = vec![[7.0; 3]; foreign.rest_normals().len()];
    let before = (v.clone(), n.clone());
    assert!(
        adapter
            .solve(
                &foreign,
                &Mat4::IDENTITY,
                &Mat4::IDENTITY,
                None,
                &mut v,
                &mut n
            )
            .unwrap_err()
            .contains("rig ajeno")
    );
    assert_eq!((v, n), before);
}
