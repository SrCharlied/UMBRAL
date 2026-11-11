//! Parent-only IO/replay regressions against frozen product; real assets read-only, disposable outputs.
use std::path::PathBuf;
use umbral::{
    app::{GameOptions, PerceptionRoute, run_game_with_tentacle_auto_capture},
    visual::{static_creature::CreatureOptions, tentacle_live_pursuit::LivePursuitRequest},
};
fn env(k: &str) -> PathBuf {
    std::env::var_os(k).expect("explicit fixture env").into()
}
fn fresh(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "gt3_parent_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn options(output: PathBuf) -> GameOptions {
    GameOptions {
        width: 64,
        height: 64,
        headless: false,
        frame_limit: None,
        output,
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: Some(CreatureOptions {
            model: env("UMBRAL_CAPTURE_CREATURE_OBJ"),
            texture: env("UMBRAL_CREATURE_TEXTURE"),
            near: false,
        }),
        creature_reveal: false,
        creature_reveal_stages: true,
        creature_eye_follow: true,
        debug_oxygen: 1.0,
    }
}
fn run(o: GameOptions, replay: Option<PathBuf>) -> Result<(), String> {
    run_game_with_tentacle_auto_capture(
        o,
        PerceptionRoute {
            solar: true,
            lucid: true,
            void: true,
        },
        true,
        true,
        LivePursuitRequest {
            capture_dir: env("UMBRAL_CAPTURE_PACKAGE_DIR"),
            growth_package: env("UMBRAL_GROWTH_PACKAGE"),
            correspondence: env("UMBRAL_ARM_CORRESPONDENCE"),
        },
        replay,
    )
}
#[test]
#[ignore = "private fixture, parent real API"]
fn parent_io_headless_does_not_replace_existing_png() {
    let d = fresh("io");
    let control = d.join("valid_control.png");
    let mut o = options(control.clone());
    o.headless = true;
    run(o, None).expect("matching valid fresh-output control");
    assert!(
        std::fs::read(control)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    let output = d.join("sentinel.png");
    let sentinel = b"PARENT_OWNED_SENTINEL_NOT_INPUT";
    std::fs::write(&output, sentinel).unwrap();
    let mut o = options(output.clone());
    o.headless = true;
    let result = run(o, None);
    let intact = std::fs::read(&output).unwrap() == sentinel;
    println!(
        "PARENT_IO01 rejected={} sentinel_intact={intact}",
        result.is_err()
    );
    assert!(
        result.is_err() && intact,
        "IO-01: existing unrelated PNG output replaced by automatic headless API"
    );
}
#[test]
#[ignore = "private fixture, parent real API"]
fn parent_replay_limited_before_done_cannot_report_success() {
    let d = fresh("replay");
    let output = d.join("replay");
    std::fs::create_dir(&output).unwrap();
    let mut o = options(d.join("p.png"));
    o.frame_limit = Some(1);
    let result = run(o, Some(output.clone()));
    let complete = output.join("metrics.json").exists();
    println!(
        "PARENT_REPLAY01 rejected={} has_metrics={complete}",
        result.is_err()
    );
    assert!(
        result.is_err(),
        "REPLAY-01: incomplete frame-limited replay returned success"
    );
}
