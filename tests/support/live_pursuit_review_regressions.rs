//! Review regressions only. Prepared without compilation/execution; no product fixes.
use super::*;

#[path = "live_pursuit_review_thick.rs"]
mod thick;

fn max_jump(a: &[V], b: &[V]) -> f64 {
    a.iter().zip(b).map(|(a, b)| (v3(*a) - v3(*b)).length()).fold(0.0, f64::max)
}

/// Snapshot every publicly observable wrapper/controller/rig/debug publication.
/// Full model Debug includes topology, UVs, materials and bounds, not only vertices.
fn publication(p: &LivePursuit) -> String {
    let rig = p.rig().map(|r| format!("{:?} {:?} {:?} {:?} {:?} {:?}",
        r.rest_loop(), r.tip_offset(), r.bend_radius(), r.exact_radius(),
        r.rest_vertices(), r.rest_normals()));
    let d = p.debug();
    format!("{:?}", (
        p.controller(), p.last(), p.composite(), p.body_prefix(), rig,
        p.body_override().map(|b| (format!("{:?}", b.model), b.extra_bounds_obj)),
        p.title(), d.bind_report(),
        d.bound_layer().map(|(m, nv, nn)| (format!("{m:?}"), nv, nn)),
        (d.composite(), d.growth_mesh(), d.is_active(), d.snapshot(),
         d.timeline(), d.clock(), d.last_frame())
    ))
}

#[test]
fn parent_review_lp_r01_near_antipode_3d_azimuth_tracks_target_step() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    let mut violations = Vec::new();
    for eps in [1e-2_f64, 1e-3, 1e-4, 1e-5] {
        let a = cp + Vec3::new(-eps.cos(), eps.sin(), 0.0);
        let b = cp + Vec3::new(-eps.cos(), 0.0, eps.sin());
        assert!((a-cp).length() >= r.exact_radius() && (b-cp).length() < MAX_REACH_OBJ);
        let (mut av, mut an) = buffers(&d);
        let pa = ok(r.deform([a.x,a.y,a.z], None, &mut av, &mut an), "azimuth Y");
        let (mut bv, mut bn) = buffers(&d);
        let pb = ok(r.deform([b.x,b.y,b.z], Some(pa.bend_side), &mut bv, &mut bn), "azimuth Z");
        let target_step = (b-a).length();
        let rings = d.package().data().topology().rings();
        let jump = rings[1..rings.len()-1].iter().flatten()
            .map(|&i| (v3(av[i])-v3(bv[i])).length()).fold(0.0, f64::max);
        // A generous local gain (8), not an absolute bound that hides a finite jump as eps -> 0.
        if jump > 8.0 * target_step + 1e-5 {
            violations.push((eps, target_step, jump, pb.side_switched));
        }
    }
    assert!(violations.is_empty(), "LP-R01 finite mesh jump for vanishing 3D target step: {violations:?}");
}

#[test]
fn parent_review_lp_r02_planar_sweep_checks_every_frame_including_switch() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    let mut violations = Vec::new();
    for subdivisions in [1_usize, 4, 20] {
        let mut previous: Option<(Vec<V>, Vec3)> = None;
        let mut side = None;
        for i in 0..=240 * subdivisions {
            let degrees = 60.0 + i as f64 / subdivisions as f64;
            let a = degrees.to_radians();
            let tip = cp + Vec3::new(a.cos(), a.sin(), 0.0);
            let (mut v, mut n) = buffers(&d);
            let pose = ok(r.deform([tip.x,tip.y,tip.z], side, &mut v, &mut n), "planar sweep");
            if let Some((pv, pt)) = &previous {
                let jump = max_jump(pv, &v);
                let step = (tip-*pt).length();
                // Deliberately no exemption for pose.side_switched.
                if jump > 8.0 * step + 1e-5 {
                    violations.push((subdivisions, degrees, pose.side_switched, step, jump));
                }
            }
            side = Some(pose.bend_side);
            previous = Some((v, tip));
        }
    }
    assert!(violations.is_empty(), "LP-R02 discontinuity, including switched frames: {violations:?}");
}

#[test]
fn parent_review_lp_r03_finite_huge_distal_vertex_outputs_finite_or_rejects_atomically() {
    let (d, healthy) = rig();
    let cp = v3(healthy.rest_loop().center);
    let tip = cp + Vec3::new(-1.0, 1.0, 0.0).normalized();
    let tip = [tip.x,tip.y,tip.z];
    let (mut cv, mut cn) = buffers(&d);
    ok(healthy.deform(tip, None, &mut cv, &mut cn), "healthy control same valid tip");
    let mut arm = tube(1).vertices;
    arm[24] = [1.7e308, 1.7e308, 0.0]; // Finite, distal, NOT in loop_p or grip.
    assert!(arm.iter().flatten().all(|x| x.is_finite()));
    let (mut v, mut n) = buffers(&d);
    let before = (v.clone(), n.clone());
    match ShaftRig::new(d.package().data(), &arm) {
        Err(_) => assert_eq!((v,n), before, "constructor rejection has no output writes"),
        Ok(r) => match r.deform(tip, None, &mut v, &mut n) {
            Err(_) => assert_eq!((v,n), before, "LP-R03 Err must preserve both buffers"),
            Ok(pose) => {
                assert!(v.iter().chain(&n).flatten().all(|x| x.is_finite()),
                    "LP-R03 Ok published nonfinite derived vertex/normal from finite input");
                assert!(pose.axial_tip.iter().chain(&pose.distal_center).chain(&pose.tip_obj)
                    .chain(&pose.bend_side).all(|x| x.is_finite()));
                assert!([pose.bend, pose.end_rotation, pose.shaft_length, pose.line_length]
                    .iter().all(|x| x.is_finite()), "LP-R03 nonfinite pose output");
            }
        }
    }
}

#[test]
fn parent_review_lp_r04_replaced_valid_creature_rejected_without_publication() {
    let (mut w, _g) = game(0.05);
    let mut p = pursuit(&w);
    for _ in 0..3 { ok(p.step(&w, STEP), "initial healthy publication"); }
    assert!(p.controller().is_some() && p.last().is_some() && p.body_override().is_some());
    let before = publication(&p);
    let placement = w.creature().unwrap().placement();
    let mut replacement = creature_model();
    replacement.vertices[129][0] += 1e-3; // Non-eye BODY vertex, no arm/cut deformation.
    replacement.update_bounds();
    let changed_geometry = geometry_fnv(&replacement);
    let creature = ok(StaticCreature::new(replacement, texture(), placement), "valid replacement");
    ok(w.set_creature(creature), "legal World replacement with refreshed partition");
    assert_eq!(geometry_fnv(w.creature().unwrap().model()), changed_geometry);
    // Independent inherited identity guard must reject this exact replacement.
    assert!(debug().bind(&w).is_err(), "control: incompatible capture identity");
    let result = p.step(&w, STEP);
    let after = publication(&p); // Capture even when the unexpected result is Ok.
    assert!(result.is_err(), "LP-R04 incompatible cached identity accepted: {result:?}; publication_unchanged={}", before == after);
    assert_eq!(after, before, "LP-R04 rejected step changed controller/composite/debug/bind status");
}

#[test]
fn parent_review_lp_r05_user_selected_no_overshoot_emergency_arrival() {
    // Charlie explicitly selected exact arrival even when braking must be immediate.
    // Original contradictory diagnostic and its native RED are archived by the parent.
    let mut c = rest();
    for _ in 0..12 {
        ok(c.step(&input(Vec3::new(900.0,0.0,0.0), 0.05, DT)), "accelerate");
    }
    assert!((c.speed()-48.0).abs() < 1e-9, "precondition speed48");
    let before = c;
    let target = c.tip() + Vec3::X * 0.01; // Exterior radial arrival; valid min/max ball.
    let report = ok(c.step(&input(target, 0.05, DT)), "near target change");
    assert!(report.reached && (c.tip()-target).length() < 1e-9, "preserve exact no-overshoot arrival");
    assert!((c.tip()-before.tip()).length() <= 0.01 + 1e-9);
    let actual_speed = (c.tip()-before.tip()).length() / DT;
    assert!((c.speed()-actual_speed).abs() < 1e-9, "arrival reports actual advance speed, not a fictitious budget");
    assert!(before.speed()-c.speed() > PURSUIT_ACCELERATION * DT,
        "this fixture must exercise the explicitly selected emergency-arrival exception");
    let held = c.tip();
    let report = ok(c.step(&input(target, 0.05, DT)), "hold target without overshoot");
    assert!(report.reached);
    assert!((c.tip()-held).length() < 1e-9 && c.speed().abs() < 1e-9,
        "stay at the current target without inventing ship attachment");
}

#[test]
fn parent_review_lp_r06_thick_fixture_inherited_bind_control_is_healthy() {
    let (w, _g) = thick::game(0.05);
    let mut d = thick::debug();
    let report = ok(d.bind(&w), "mandatory healthy inherited bind radius .09");
    assert!(report.max_position_error <= 1e-9 && report.arm_faces_removed > 0);
    assert!(d.bind_report().is_some() && d.bound_layer().is_some());
    let arm = thick::tube(1).vertices;
    let error = ShaftRig::new(d.package().data(), &arm).err().expect("thick rig must reject");
    assert!(error.contains("reposo invalido para el codo"), "wrong failure stage: {error}");
    let root = root_center();
    assert!((arm[0][1] - root.y - 0.09).abs() < 1e-12, "fixture radius .09");
}

#[test]
fn parent_review_lp_r06_failed_live_bind_preserves_debug_and_wrapper_state() {
    let (w, _g) = thick::game(0.05);
    // Repeat control within this regression, so filtering to this name cannot bypass it.
    let mut control = thick::debug();
    ok(control.bind(&w), "mandatory inherited healthy control");
    let error = ShaftRig::new(control.package().data(), &thick::tube(1).vertices)
        .err().expect("thick rig must reject after inherited success");
    assert!(error.contains("reposo invalido para el codo"), "wrong failure stage: {error}");
    let mut p = LivePursuit::new(thick::debug());
    let before = publication(&p);
    let result = p.bind(&w);
    let after = publication(&p);
    assert!(result.is_err(), "thick live rig must reject");
    assert_eq!(after, before, "LP-R06 Err partially published debug bind_report/bound_layer or wrapper state");
}

#[test]
fn parent_review_lp_r07_huge_finite_side_is_unit_y_equivalent_or_atomic_error() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    let tip = cp - Vec3::X * 1.5; // Valid rear target, beyond exact ball and below max12.
    assert!(1.5 >= r.exact_radius() && 1.5 < MAX_REACH_OBJ);
    let tip = [tip.x,tip.y,tip.z];
    let (mut reference_v, mut reference_n) = buffers(&d);
    ok(r.deform(tip, Some([0.0,1.0,0.0]), &mut reference_v, &mut reference_n), "unit Y control");
    let (mut v, mut n) = buffers(&d);
    let before = (v.clone(), n.clone());
    match r.deform(tip, Some([0.0,1e308,0.0]), &mut v, &mut n) {
        Err(_) => assert_eq!((v,n), before, "LP-R07 Err writes neither buffer"),
        Ok(_) => {
            let finite = v.iter().chain(&n).flatten().all(|x| x.is_finite());
            let unit_normals = n.iter().all(|x| (v3(*x).length()-1.0).abs() < 1e-6);
            let rigid_rings = d.package().data().topology().rings().iter().all(|ring| {
                ring.iter().enumerate().all(|(i, &a)| ring.iter().skip(i+1).all(|&b| {
                    ((v3(v[a])-v3(v[b])).length() -
                     (v3(reference_v[a])-v3(reference_v[b])).length()).abs() < 1e-9
                }))
            });
            let geometry_error = max_jump(&v, &reference_v);
            let normal_error = max_jump(&n, &reference_n);
            assert!(finite && unit_normals && rigid_rings && geometry_error < 1e-9 && normal_error < 1e-9,
                "LP-R07 overflowing side norm: finite={finite}, unit_normals={unit_normals}, rigid_rings={rigid_rings}, geometry_error={geometry_error}, normal_error={normal_error}");
        }
    }
}
