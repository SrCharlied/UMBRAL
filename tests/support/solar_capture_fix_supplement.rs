fn alias_api(kind: &str) {
    use umbral::app::{GameOptions, PerceptionRoute, run_game_with_tentacle_capture};
    use umbral::visual::static_creature::CreatureOptions;
    let dir = std::env::temp_dir().join(format!(
        "fix01_{kind}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let key = if kind == "model" {
        "UMBRAL_CAPTURE_CREATURE_OBJ"
    } else {
        "UMBRAL_CREATURE_TEXTURE"
    };
    let protected = dir.join(if kind == "model" {
        "input.obj"
    } else {
        "input.png"
    });
    if kind == "model" {
        let model = env_path(key);
        let text = std::fs::read_to_string(&model).unwrap();
        for line in text.lines().filter_map(|s| s.strip_prefix("mtllib ")) {
            let name = std::path::Path::new(line);
            assert!(name.components().count() == 1);
            std::fs::copy(model.parent().unwrap().join(name), dir.join(name)).unwrap();
        }
    }
    std::fs::copy(env_path(key), &protected).unwrap();
    let before = std::fs::read(&protected).unwrap();
    let output = dir.join("out.png");
    std::fs::hard_link(&protected, &output).unwrap();
    let result = run_game_with_tentacle_capture(
        GameOptions {
            width: 96,
            height: 54,
            headless: true,
            frame_limit: Some(1),
            output,
            time: 0.0,
            near: None,
            culling: true,
            lod: true,
            overlay: false,
            creature: Some(CreatureOptions {
                model: if kind == "model" {
                    protected.clone()
                } else {
                    env_path("UMBRAL_CAPTURE_CREATURE_OBJ")
                },
                texture: if kind == "texture" {
                    protected.clone()
                } else {
                    env_path("UMBRAL_CREATURE_TEXTURE")
                },
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
        real_request(),
    );
    assert!(
        before == std::fs::read(&protected).unwrap(),
        "hardlink changed copied {kind}"
    );
    assert!(result.is_err(), "alias must reject before render");
}

#[test]
#[ignore = "private assets, disposable copies only"]
fn fix_sc01_model_hardlink() {
    alias_api("model");
}

#[test]
#[ignore = "private assets, disposable copies only"]
fn fix_sc01_texture_hardlink() {
    alias_api("texture");
}

use super::*;

fn copy(m: &Model) -> Model {
    Model {
        vertices: m.vertices.clone(),
        uvs: m.uvs.clone(),
        normals: m.normals.clone(),
        faces: m
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: m.material_libraries.clone(),
        materials: m.materials.clone(),
        bounds_center: m.bounds_center,
        bounds_radius: m.bounds_radius,
    }
}

#[test]
fn fix_sc02_finite_mvp_can_overflow_vertex_before_clear() {
    let (mut w, _, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    d.activate(&w).unwrap();
    d.update_frame(&w).unwrap();
    w.camera.position = Vec3::ZERO;
    w.camera.target = -Vec3::Z;
    w.camera.up = Vec3::Y;
    w.camera.set_lens(2e-160, 1.0, 0.1, 100.0).unwrap();
    let mut m = copy(d.composite().unwrap());
    let i = w.staged_creature_parts().unwrap().body().vertices.len();
    m.vertices[i] = [1e150, 0.0, 0.0];
    let mvp = w.camera.view_projection() * w.creature_render_transform().unwrap().matrix();
    assert!(mvp.m.iter().flatten().all(|v| v.is_finite()));
    assert!(
        !mvp.transform(Vec3::from_array(m.vertices[i]).extend(1.0))
            .x
            .is_finite()
    );
    let o = SolarBodyOverride {
        model: &m,
        extra_bounds_obj: (Vec3::ZERO, 1e151),
    };
    let mut fb = seeded();
    let (pixels, depth) = (fb.pixels.clone(), depth_signature(&fb));
    assert!(
        w.render_solar_reveal_preview_with(&mut fb, Some(&o))
            .is_err(),
        "finite MVP overflowed actual vertex"
    );
    assert_eq!(pixels, fb.pixels);
    assert_eq!(depth, depth_signature(&fb));
}
