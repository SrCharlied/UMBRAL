// Parent-owned review regressions. Append-only after the complete original test file.
use super::*;

#[test]
fn parent_sc02_composed_matrix_overflow_rejects_before_clear() {
    let (mut w, _, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.set_clock(6.9).unwrap();
    d.update_frame(&w).unwrap();
    w.camera.position = Vec3::ZERO;
    w.camera.target = Vec3::new(0.0, 0.0, -1.0);
    w.camera.up = Vec3::Y;
    w.camera.set_lens(2e-308, 1.0, 0.1, 100.0).unwrap();
    let finite = |m: &Mat4| m.m.iter().flatten().all(|v| v.is_finite());
    let vp = w.camera.view_projection();
    let model = w.creature_render_transform().unwrap().matrix();
    assert!(finite(&vp) && finite(&model), "finite individual matrices");
    assert!(!finite(&(vp * model)), "fixture must overflow composition");
    let o = d.body_override().unwrap();
    let mut fb = seeded();
    let (pixels, depth) = (fb.pixels.clone(), depth_signature(&fb));
    let result = w.render_solar_reveal_preview_with(&mut fb, Some(&o));
    assert!(
        result.is_err(),
        "SC02: nonfinite MVP must reject before clear"
    );
    assert!(fb.pixels == pixels && depth_signature(&fb) == depth);
}

#[test]
fn parent_sc03_failed_update_keeps_every_published_mesh() {
    let (mut w, _, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.set_clock(3.0).unwrap();
    d.update_frame(&w).unwrap();
    d.set_clock(6.9).unwrap();
    let growth = (
        d.growth_mesh().vertices.clone(),
        d.growth_mesh().normals.clone(),
    );
    let composite = d.composite().unwrap().vertices.clone();
    let status = d.last_frame();
    let bounds = d.body_override().unwrap().extra_bounds_obj;
    let clock = d.clock();
    w.clear_creature();
    assert!(d.update_frame(&w).is_err());
    assert_eq!(
        growth.0,
        d.growth_mesh().vertices,
        "SC03: rejected update published growth vertices"
    );
    assert_eq!(growth.1, d.growth_mesh().normals);
    assert_eq!(composite, d.composite().unwrap().vertices);
    assert_eq!(status, d.last_frame());
    assert_eq!(bounds, d.body_override().unwrap().extra_bounds_obj);
    assert_eq!(clock, d.clock());
}

#[test]
fn parent_sc04_external_max_count_returns_error_without_panic() {
    let text = texts()
        .2
        .lines()
        .map(|line| {
            if line.starts_with("counts ") {
                format!("counts {} 1 {} 1", usize::MAX, usize::MAX)
            } else if line.starts_with("arm_vertex ") {
                "arm_vertex".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let result = std::panic::catch_unwind(|| ArmCorrespondence::parse(&text));
    assert!(result.is_ok(), "SC04: untrusted external count panicked");
    assert!(result.unwrap().is_err());
}

#[test]
fn parent_sc05_hidden_eye_still_validates_some_override() {
    let (mut w, _, mut d) = bound(1.0);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.update_frame(&w).unwrap();
    let bad = SolarBodyOverride {
        model: d.composite().unwrap(),
        extra_bounds_obj: (Vec3::new(f64::NAN, 0.0, 0.0), 1.0),
    };
    let mut fb = seeded();
    let (pixels, depth) = (fb.pixels.clone(), depth_signature(&fb));
    assert!(
        w.render_solar_reveal_preview_with(&mut fb, Some(&bad))
            .is_err(),
        "SC05: hidden eye bypassed validation"
    );
    assert!(fb.pixels == pixels && depth_signature(&fb) == depth);
}

#[test]
fn parent_sc05_missing_solar_route_rejects_some_override() {
    let (mut w, _, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.update_frame(&w).unwrap();
    let plain = World::new(ship_model(), W as f64 / H as f64).unwrap();
    let o = d.body_override().unwrap();
    let mut fb = seeded();
    let (pixels, depth) = (fb.pixels.clone(), depth_signature(&fb));
    assert!(
        plain
            .render_solar_reveal_preview_with(&mut fb, Some(&o))
            .is_err(),
        "SC05: missing solar route silently rendered"
    );
    assert!(fb.pixels == pixels && depth_signature(&fb) == depth);
}

#[test]
fn parent_extra_sphere_must_contain_appended_geometry() {
    let (mut w, _, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.set_clock(6.9).unwrap();
    d.update_frame(&w).unwrap();
    let bad = SolarBodyOverride {
        model: d.composite().unwrap(),
        extra_bounds_obj: (Vec3::ZERO, 0.0),
    };
    let mut fb = seeded();
    let pixels = fb.pixels.clone();
    assert!(
        w.render_solar_reveal_preview_with(&mut fb, Some(&bad))
            .is_err(),
        "external sphere omitted the appendix"
    );
    assert_eq!(pixels, fb.pixels);
}

#[test]
fn parent_extra_public_timeline_checks_distance_and_base_windows() {
    let d = debug();
    let good = capture_base_timing(d.package().capture().manifest()).unwrap();
    assert!(
        CaptureTimeline::new(good, FPS, FRAMES, -1.0, 1.0).is_err(),
        "negative distance accepted"
    );
    let mut nan = good;
    nan.reach.0 = f64::NAN;
    assert!(CaptureTimeline::new(nan, FPS, FRAMES, 1.0, 1.0).is_err());
    let mut backwards = good;
    backwards.reach = (good.reach.1, good.reach.0);
    assert!(CaptureTimeline::new(backwards, FPS, FRAMES, 1.0, 1.0).is_err());
}

// Preservation guard expected to start GREEN; not counted as a missing-behavior RED.
#[test]
fn parent_clock_addition_overflow_keeps_clock_and_pose() {
    let (mut w, _, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.update_frame(&w).unwrap();
    d.set_clock(f64::MAX).unwrap();
    let clock = d.clock();
    let vertices = d.growth_mesh().vertices.clone();
    assert!(d.advance(f64::MAX).is_err());
    assert_eq!(clock, d.clock());
    assert_eq!(vertices, d.growth_mesh().vertices);
}

fn parent_disposable_dir() -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("umbral_sc01_{}_{stamp}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    p
}

fn parent_copy_tree(source: &std::path::Path, dest: &std::path::Path) {
    std::fs::create_dir(dest).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let p = entry.path();
        let q = dest.join(entry.file_name());
        assert!(
            !entry.file_type().unwrap().is_symlink(),
            "fixture does not copy through symlinks"
        );
        if p.is_dir() {
            parent_copy_tree(&p, &q);
        } else {
            std::fs::copy(p, q).unwrap();
        }
    }
}

fn parent_sc01_output_alias(kind: &str, cli: bool) {
    use umbral::app::{GameOptions, PerceptionRoute, run_game_with_tentacle_capture};
    use umbral::visual::static_creature::CreatureOptions;
    let dir = parent_disposable_dir();
    let mut request = real_request();
    let protected = match kind {
        "growth" => {
            let p = dir.join("growth_package.txt");
            std::fs::copy(&request.growth_package, &p).unwrap();
            request.growth_package = p.clone();
            p
        }
        "correspondence" => {
            let p = dir.join("correspondence.txt");
            std::fs::copy(&request.correspondence, &p).unwrap();
            request.correspondence = p.clone();
            p
        }
        "capture_manifest" | "capture_obj" => {
            let p = dir.join("capture");
            parent_copy_tree(&request.capture_dir, &p);
            request.capture_dir = p.clone();
            if kind == "capture_manifest" {
                p.join("capture_package.txt")
            } else {
                let text = std::fs::read_to_string(p.join("capture_package.txt")).unwrap();
                let name = text
                    .lines()
                    .find(|s| s.starts_with("file arm "))
                    .unwrap()
                    .split_whitespace()
                    .nth(2)
                    .unwrap();
                let relative = std::path::Path::new(name);
                assert!(
                    !relative.is_absolute()
                        && !relative
                            .components()
                            .any(|c| matches!(c, std::path::Component::ParentDir))
                );
                p.join(relative)
            }
        }
        _ => panic!("unknown disposable fixture kind"),
    };
    let before = std::fs::read(&protected).unwrap();
    let output = dir.join("output.png");
    std::fs::hard_link(&protected, &output).unwrap();
    let result = if cli {
        let o = Command::new(env!("CARGO_BIN_EXE_umbral"))
            .args([
                "--headless",
                "--creature-solar-reveal",
                "--tentacle-capture-debug",
                "--creature-model",
            ])
            .arg(env_path("UMBRAL_CAPTURE_CREATURE_OBJ"))
            .arg("--creature-texture")
            .arg(env_path("UMBRAL_CREATURE_TEXTURE"))
            .arg("--tentacle-capture-package")
            .arg(&request.capture_dir)
            .arg("--tentacle-growth-package")
            .arg(&request.growth_package)
            .arg("--tentacle-arm-correspondence")
            .arg(&request.correspondence)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        if o.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).into_owned())
        }
    } else {
        run_game_with_tentacle_capture(
            GameOptions {
                width: 96,
                height: 54,
                headless: true,
                frame_limit: Some(1),
                output: output.clone(),
                time: 0.0,
                near: None,
                culling: true,
                lod: true,
                overlay: false,
                creature: Some(CreatureOptions {
                    model: env_path("UMBRAL_CAPTURE_CREATURE_OBJ"),
                    texture: env_path("UMBRAL_CREATURE_TEXTURE"),
                    near: false,
                }),
                creature_reveal: false,
                creature_reveal_stages: true,
                creature_eye_follow: true,
                debug_oxygen: 0.05,
            },
            PerceptionRoute {
                solar: true,
                lucid: false,
                void: false,
            },
            false,
            false,
            request,
        )
    };
    let after = std::fs::read(&protected).unwrap();
    println!(
        "SC01 disposable {} kind={kind} cli={cli} protected_before_fnv={:016x} protected_after_fnv={:016x} rejected={}",
        dir.display(),
        fnv1a64(&before),
        fnv1a64(&after),
        result.is_err()
    );
    assert!(
        before == after,
        "SC01: output truncated a DISPOSABLE copied input: {kind}, cli={cli}"
    );
    assert!(result.is_err(), "SC01: input/output alias was not rejected");
}

#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_api_growth_alias() {
    parent_sc01_output_alias("growth", false);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_api_correspondence_alias() {
    parent_sc01_output_alias("correspondence", false);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_api_capture_manifest_alias() {
    parent_sc01_output_alias("capture_manifest", false);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_api_capture_obj_alias() {
    parent_sc01_output_alias("capture_obj", false);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_cli_growth_alias() {
    parent_sc01_output_alias("growth", true);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_cli_correspondence_alias() {
    parent_sc01_output_alias("correspondence", true);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_cli_capture_manifest_alias() {
    parent_sc01_output_alias("capture_manifest", true);
}
#[test]
#[ignore = "private assets, disposable copies only"]
fn parent_sc01_cli_capture_obj_alias() {
    parent_sc01_output_alias("capture_obj", true);
}
