// SC01-MTL: real CLI/API, only disposable copies can be writer targets.
use super::*;
use std::path::{Component, Path, PathBuf};
use umbral::app::{GameOptions, PerceptionRoute, run_game_with_tentacle_capture};
use umbral::visual::static_creature::CreatureOptions;

struct Fixture {
    dir: PathBuf,
    model: PathBuf,
    texture: PathBuf,
    mtls: Vec<PathBuf>,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "sc01_mtl_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let original = env_path("UMBRAL_CAPTURE_CREATURE_OBJ");
        let source = std::fs::read_to_string(&original).unwrap();
        let parsed = umbral::assets::obj::parse_obj(&source).unwrap();
        assert!(!parsed.material_libraries.is_empty());
        let mut mtls = Vec::new();
        // Multiple declarations and nested Normal components; .png permits same-path
        // alias without an unrelated extension rejection. MTL content stays valid.
        let mut obj = source
            .lines()
            .filter(|l| l.split('#').next().unwrap().split_whitespace().next() != Some("mtllib"))
            .collect::<Vec<_>>()
            .join("\n");
        for (i, name) in parsed.material_libraries.iter().enumerate() {
            let relative = Path::new(name);
            assert!(
                relative
                    .components()
                    .all(|p| matches!(p, Component::Normal(_)))
            );
            let bytes = std::fs::read(original.parent().unwrap().join(relative)).unwrap();
            for suffix in ["first", "second"] {
                let name = format!("materials/{i}_{suffix}.png");
                let path = dir.join(&name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, &bytes).unwrap();
                obj.push_str(&format!("\nmtllib {name} # parser comment\n"));
                mtls.push(path);
            }
        }
        let model = dir.join("creature.obj");
        std::fs::write(&model, obj).unwrap();
        let texture = dir.join("texture.png");
        std::fs::copy(env_path("UMBRAL_CREATURE_TEXTURE"), &texture).unwrap();
        // Fixture validity is independently checked through the frozen actual loader.
        umbral::assets::obj::load_model(&model).unwrap();
        Self {
            dir,
            model,
            texture,
            mtls,
        }
    }

    fn run(&self, cli: bool, output: &Path) -> Result<(), String> {
        let request = real_request();
        if cli {
            let o = Command::new(env!("CARGO_BIN_EXE_umbral"))
                .args([
                    "--headless",
                    "--width",
                    "96",
                    "--height",
                    "54",
                    "--creature-solar-reveal",
                    "--debug-oxygen",
                    "0.05",
                    "--tentacle-capture-debug",
                    "--creature-model",
                ])
                .arg(&self.model)
                .arg("--creature-texture")
                .arg(&self.texture)
                .arg("--tentacle-capture-package")
                .arg(&request.capture_dir)
                .arg("--tentacle-growth-package")
                .arg(&request.growth_package)
                .arg("--tentacle-arm-correspondence")
                .arg(&request.correspondence)
                .arg("--output")
                .arg(output)
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
                    output: output.to_path_buf(),
                    time: 0.0,
                    near: None,
                    culling: true,
                    lod: true,
                    overlay: false,
                    creature: Some(CreatureOptions {
                        model: self.model.clone(),
                        texture: self.texture.clone(),
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
        }
    }
}

fn mtl_case(cli: bool, alias: Option<bool>) {
    let fixture = Fixture::new();
    let before: Vec<_> = fixture
        .mtls
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    let target = fixture.mtls.last().unwrap();
    let output = match alias {
        Some(true) => target.clone(),
        Some(false) => {
            let p = fixture.dir.join("hardlink.png");
            std::fs::hard_link(target, &p).unwrap();
            p
        }
        None => fixture.dir.join("valid.png"),
    };
    let result = fixture.run(cli, &output);
    if alias.is_some() {
        assert!(
            result.is_err(),
            "SC01-MTL: alias was accepted and writer ran: {result:?}"
        );
        let error = result.unwrap_err();
        assert!(
            error.contains("--output") && (error.contains("entrada") || error.contains("distinto")),
            "unexpected rejection: {error}"
        );
    } else {
        result.unwrap();
        assert!(
            std::fs::read(&output)
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
        // Preservation control: saving again to an existing valid PNG remains allowed.
        let png = std::fs::read(&output).unwrap();
        fixture.run(cli, &output).unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), png);
    }
    for (p, bytes) in fixture.mtls.iter().zip(before) {
        assert_eq!(std::fs::read(p).unwrap(), bytes, "disposable MTL mutated");
    }
    std::fs::remove_dir_all(&fixture.dir).unwrap();
}

#[test]
#[ignore = "real private inputs; writes only copies"]
fn sc01_mtl_cli_control_valid_png() {
    mtl_case(true, None);
}
#[test]
#[ignore = "real private inputs; writes only copies"]
fn sc01_mtl_api_control_valid_png() {
    mtl_case(false, None);
}
#[test]
#[ignore = "real private inputs; writes only copies"]
fn sc01_mtl_cli_hardlink() {
    mtl_case(true, Some(false));
}
#[test]
#[ignore = "real private inputs; writes only copies"]
fn sc01_mtl_api_hardlink() {
    mtl_case(false, Some(false));
}
#[test]
#[ignore = "real private inputs; writes only copies"]
fn sc01_mtl_cli_same_path() {
    mtl_case(true, Some(true));
}
#[test]
#[ignore = "real private inputs; writes only copies"]
fn sc01_mtl_api_same_path() {
    mtl_case(false, Some(true));
}
