//! CORRECTION03 (fixture privado, opt-in; PRESERVACION nacida en verde, probada por mutacion en copia):
//! las guardas IO/replay de la captura automatica tambien por la entrada REAL de la CLI
//! (`ConfiguredGameOptions` con franja opcional y origen explicito de O2) y por el binario.
use std::{path::PathBuf, process::Command};
use umbral::{
    app::{
        ConfiguredGameOptions, GameOptions, OxygenSource, PerceptionRoute,
        run_game_with_tentacle_auto_capture,
    },
    visual::{static_creature::CreatureOptions, tentacle_live_pursuit::LivePursuitRequest},
};
fn env(k: &str) -> PathBuf {
    std::env::var_os(k).expect("explicit fixture env").into()
}
fn fresh(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "c3_io_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn configured(output: PathBuf) -> ConfiguredGameOptions {
    ConfiguredGameOptions {
        options: GameOptions {
            width: 64,
            height: 64,
            headless: true,
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
        },
        gameplay_feedback: true,
        oxygen_source: OxygenSource::Debug,
        oxygen_seconds: None,
        presentation: false,
        oxygen_hud: Default::default(),
        start_menu: false,
    }
}
fn run(o: ConfiguredGameOptions, replay: Option<PathBuf>) -> Result<(), String> {
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
fn cli_auto(extra: &[std::ffi::OsString]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_umbral"));
    cmd.arg("--creature-solar-reveal")
        .arg("--lucid-surface")
        .arg("--void-eye")
        .arg("--watching-stars")
        .arg("--creature-model")
        .arg(env("UMBRAL_CAPTURE_CREATURE_OBJ"))
        .arg("--creature-texture")
        .arg(env("UMBRAL_CREATURE_TEXTURE"))
        .arg("--tentacle-auto-capture")
        .arg("--tentacle-capture-package")
        .arg(env("UMBRAL_CAPTURE_PACKAGE_DIR"))
        .arg("--tentacle-growth-package")
        .arg(env("UMBRAL_GROWTH_PACKAGE"))
        .arg("--tentacle-arm-correspondence")
        .arg(env("UMBRAL_ARM_CORRESPONDENCE"))
        .arg("--gameplay-feedback")
        .args(["--debug-oxygen", "1"])
        .args(extra);
    cmd.output().unwrap()
}

#[test]
#[ignore = "private fixture; correction03 configured IO"]
fn c3_configured_api_keeps_exclusive_output_alias_and_replay_guards() {
    let d = fresh("api");
    // Control positivo: salida nueva con la entrada configurada.
    let control = d.join("control.png");
    run(configured(control.clone()), None).expect("control positivo configurado");
    assert!(
        std::fs::read(&control)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    // Salida existente ajena: no se reemplaza.
    let sentinel = d.join("sentinel.png");
    std::fs::write(&sentinel, b"C3_SENTINEL").unwrap();
    assert!(run(configured(sentinel.clone()), None).is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"C3_SENTINEL");
    // Alias (enlace duro) de una entrada protegida: rechazado, bytes intactos.
    let texture = env("UMBRAL_CREATURE_TEXTURE");
    let original = std::fs::read(&texture).unwrap();
    let alias = d.join("alias.png");
    std::fs::hard_link(&texture, &alias).unwrap();
    let error = run(configured(alias.clone()), None).unwrap_err();
    assert!(error.contains("alias de entrada protegida"), "{error}");
    assert!(std::fs::read(&alias).unwrap() == original);
    // Replay con limite de cuadros (truncado): rechazado sin escribir nada.
    let replay = d.join("replay");
    std::fs::create_dir(&replay).unwrap();
    let mut limited = configured(d.join("p.png"));
    limited.options.headless = false;
    limited.options.frame_limit = Some(1);
    let error = run(limited, Some(replay.clone())).unwrap_err();
    assert!(error.contains("no se combina con --frames"), "{error}");
    assert_eq!(std::fs::read_dir(&replay).unwrap().count(), 0);
    // Replay sobre carpeta NO vacia: rechazado, centinela intacto.
    std::fs::write(replay.join("keep"), b"K").unwrap();
    let mut full = configured(d.join("p.png"));
    full.options.headless = false;
    let error = run(full, Some(replay.clone())).unwrap_err();
    assert!(error.contains("carpeta VACIA"), "{error}");
    assert_eq!(std::fs::read(replay.join("keep")).unwrap(), b"K");
    assert!(!d.join("p.png").exists());
}

#[test]
#[ignore = "private fixture; correction03 configured CLI IO"]
fn c3_cli_with_feedback_and_explicit_oxygen_keeps_io_guards() {
    let d = fresh("cli");
    let sentinel = d.join("sentinel.png");
    std::fs::write(&sentinel, b"C3_CLI_SENTINEL").unwrap();
    let out = cli_auto(&[
        "--headless".into(),
        "--output".into(),
        sentinel.clone().into(),
    ]);
    assert!(
        !out.status.success(),
        "la CLI reemplazo una salida existente"
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"C3_CLI_SENTINEL");
    let replay = d.join("replay");
    std::fs::create_dir(&replay).unwrap();
    let out = cli_auto(&[
        "--auto-capture-replay".into(),
        replay.clone().into(),
        "--frames".into(),
        "1".into(),
    ]);
    assert!(
        !out.status.success(),
        "replay truncado por --frames informo exito"
    );
    assert!(!replay.join("metrics.json").exists());
    let fresh_png = d.join("fresh.png");
    let out = cli_auto(&[
        "--headless".into(),
        "--output".into(),
        fresh_png.clone().into(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        std::fs::read(&fresh_png)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
}
