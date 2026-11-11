//! GT3-AUTO en el juego principal: guardas de linea de comandos y de API ANTES de cualquier IO (sin assets),
//! cuadro inicial headless de la ruta automatica identico a la ruta aceptada, y replay determinista por el
//! MISMO bucle de `app` (assets privados opt-in, `#[ignore]`).
use std::path::{Path, PathBuf};
use umbral::{
    app::{
        GameOptions, PerceptionRoute, run_game_with_tentacle_auto_capture,
        run_game_with_watching_stars,
    },
    visual::{
        static_creature::CreatureOptions, tentacle_auto_capture::validate_auto_capture_flags,
        tentacle_live_pursuit::LivePursuitRequest, tentacle_solar_game::TentacleCaptureFlags,
    },
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gt3_auto_app_{name}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn flags(solar: bool, packages: bool) -> TentacleCaptureFlags {
    let p = |s: &str| packages.then(|| PathBuf::from(s));
    TentacleCaptureFlags {
        enabled: false,
        capture_dir: p("capture_v2"),
        growth_package: p("growth.txt"),
        correspondence: p("corr.txt"),
        start_time: None,
        solar,
        legacy: false,
    }
}
fn options(output: PathBuf, creature: Option<CreatureOptions>) -> GameOptions {
    GameOptions {
        width: 320,
        height: 180,
        headless: false,
        frame_limit: None,
        output,
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature,
        creature_reveal: false,
        creature_reveal_stages: true,
        creature_eye_follow: true,
        debug_oxygen: 1.0,
    }
}
fn solar_route() -> PerceptionRoute {
    PerceptionRoute {
        lucid: true,
        void: true,
        solar: true,
    }
}
fn missing_request(dir: &Path) -> LivePursuitRequest {
    LivePursuitRequest {
        capture_dir: dir.join("no_existe"),
        growth_package: dir.join("no_existe.txt"),
        correspondence: dir.join("no_existe2.txt"),
    }
}

#[test]
fn c01_flag_combinations_without_files() {
    assert_eq!(
        validate_auto_capture_flags(false, false, false, false, &flags(true, true)),
        Ok(None)
    );
    assert!(
        validate_auto_capture_flags(false, true, false, false, &flags(true, true)).is_err(),
        "replay sin captura"
    );
    assert!(
        validate_auto_capture_flags(true, false, true, false, &flags(true, true)).is_err(),
        "con espacial"
    );
    assert!(
        validate_auto_capture_flags(true, false, false, true, &flags(true, true)).is_err(),
        "con viva"
    );
    assert!(
        validate_auto_capture_flags(true, false, false, false, &flags(false, true)).is_err(),
        "sin Sol"
    );
    assert!(
        validate_auto_capture_flags(true, false, false, false, &flags(true, false)).is_err(),
        "sin paquetes"
    );
    let mut f = flags(true, true);
    f.enabled = true;
    assert!(
        validate_auto_capture_flags(true, false, false, false, &f).is_err(),
        "con F7"
    );
    let mut f = flags(true, true);
    f.start_time = Some(1.0);
    assert!(
        validate_auto_capture_flags(true, false, false, false, &f).is_err(),
        "con reloj"
    );
    let mut f = flags(true, true);
    f.legacy = true;
    assert!(
        validate_auto_capture_flags(true, false, false, false, &f).is_err(),
        "legacy"
    );
    let ok = validate_auto_capture_flags(true, true, false, false, &flags(true, true))
        .unwrap()
        .unwrap();
    assert_eq!(ok.capture_dir, PathBuf::from("capture_v2"));
}

#[test]
fn c02_api_guards_reject_before_any_io() {
    let dir = scratch("guards");
    let out = dir.join("salida.png");
    let creature = || {
        Some(CreatureOptions {
            model: dir.join("no.obj"),
            texture: dir.join("no.png"),
            near: false,
        })
    };
    let run =
        |o: GameOptions, route: PerceptionRoute, perception: bool, replay: Option<PathBuf>| {
            run_game_with_tentacle_auto_capture(
                o,
                route,
                perception,
                perception,
                missing_request(&dir),
                replay,
            )
        };
    // Sin Sol, sin percepcion, salida no PNG, paquetes inexistentes: Err y nada escrito.
    assert!(
        run(
            options(out.clone(), creature()),
            PerceptionRoute::default(),
            true,
            None
        )
        .is_err()
    );
    assert!(run(options(out.clone(), creature()), solar_route(), false, None).is_err());
    assert!(
        run(
            options(dir.join("salida.txt"), creature()),
            solar_route(),
            true,
            None
        )
        .is_err()
    );
    let e = run(options(out.clone(), creature()), solar_route(), true, None).unwrap_err();
    assert!(!e.is_empty());
    // Replay: carpeta inexistente, no vacia o con headless.
    assert!(
        run(
            options(out.clone(), creature()),
            solar_route(),
            true,
            Some(dir.join("nada"))
        )
        .is_err()
    );
    let full = dir.join("llena");
    std::fs::create_dir_all(&full).unwrap();
    std::fs::write(full.join("x"), b"x").unwrap();
    assert!(
        run(
            options(out.clone(), creature()),
            solar_route(),
            true,
            Some(full.clone())
        )
        .is_err()
    );
    let mut headless = options(out.clone(), creature());
    headless.headless = true;
    let empty = dir.join("vacia");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(run(headless, solar_route(), true, Some(empty.clone())).is_err());
    assert!(!out.exists(), "una guarda escribio la salida");
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
    assert_eq!(std::fs::read(full.join("x")).unwrap(), b"x");
    std::fs::remove_dir_all(dir).unwrap();
}

fn env(k: &str) -> PathBuf {
    PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k} (assets privados)")))
}
fn private_creature() -> Option<CreatureOptions> {
    Some(CreatureOptions {
        model: env("UMBRAL_CAPTURE_CREATURE_OBJ"),
        texture: env("UMBRAL_CREATURE_TEXTURE"),
        near: false,
    })
}
fn private_request() -> LivePursuitRequest {
    LivePursuitRequest {
        capture_dir: env("UMBRAL_CAPTURE_PACKAGE_DIR"),
        growth_package: env("UMBRAL_GROWTH_PACKAGE"),
        correspondence: env("UMBRAL_ARM_CORRESPONDENCE"),
    }
}

#[test]
#[ignore = "assets privados"]
fn c03_headless_initial_frame_equals_accepted_route_at_healthy_oxygen() {
    let dir = scratch("inicial");
    let (a, b) = (dir.join("auto.png"), dir.join("aceptada.png"));
    let mut oa = options(a.clone(), private_creature());
    oa.headless = true;
    let mut ob = options(b.clone(), private_creature());
    ob.headless = true;
    run_game_with_tentacle_auto_capture(oa, solar_route(), true, true, private_request(), None)
        .unwrap();
    run_game_with_watching_stars(ob, solar_route()).unwrap();
    let (pa, pb) = (std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    assert!(
        pa == pb,
        "el cuadro inicial de la ruta automatica no es el de la ruta aceptada"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

fn replay(name: &str) -> (Vec<u8>, Vec<(String, Vec<u8>)>) {
    let dir = scratch(name);
    let out = dir.join("replay");
    std::fs::create_dir_all(&out).unwrap();
    run_game_with_tentacle_auto_capture(
        options(dir.join("p.png"), private_creature()),
        solar_route(),
        true,
        true,
        private_request(),
        Some(out.clone()),
    )
    .unwrap();
    let metrics = std::fs::read(out.join("metrics.json")).unwrap();
    let mut pngs: Vec<(String, Vec<u8>)> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    pngs.sort();
    std::fs::remove_dir_all(dir).unwrap();
    (metrics, pngs)
}

#[test]
#[ignore = "assets privados"]
fn c04_replay_through_main_loop_is_complete_and_deterministic() {
    let (m1, p1) = replay("a");
    let text = String::from_utf8(m1.clone()).unwrap();
    for mark in [
        "arranque sano",
        "contacto real",
        "Gripped",
        "retenida",
        "suelta",
        "re-armado",
    ] {
        assert!(text.contains(mark), "replay sin la marca {mark}");
    }
    for label in [
        "inicio_O2_100",
        "contacto",
        "traspaso_B1",
        "cierre_P1",
        "agarre_bloqueo",
        "retenida",
        "suelta_O2",
        "rearmado",
    ] {
        assert!(
            p1.iter().any(|(n, _)| n.contains(label)),
            "replay sin la captura {label}"
        );
    }
    assert!(!text.contains("NaN") && !text.contains("inf"));
    let (m2, p2) = replay("b");
    assert!(m1 == m2, "metrics.json no determinista");
    assert!(p1 == p2, "PNG del replay no deterministas");
}

#[path = "support/io_replay_guards.rs"]
mod io_replay_guards;
