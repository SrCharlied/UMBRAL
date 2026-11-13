//! HUD-01 (hud_oxygen_01/correction01): las entradas PUBLICAS de la biblioteca rechazan opciones invalidas del HUD
//! de oxigeno ANTES de su primer IO (resolver rutas de entrada/salida, carpeta de replay o cargar el modelo). Cada caso
//! usa opciones por lo demas VALIDAS hasta ese IO (el control con el HUD valido llega a la validacion de rutas) y un
//! PNG de salida propio que existe y jamas debe cambiar. Equivalente de biblioteca del probe del padre
//! (parent_review/hud_preflight_parent.rs), extendido a todas las entradas publicas que validan rutas.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use umbral::{
    app::{
        ConfiguredGameOptions, GameOptions, OxygenSource, PerceptionRoute, run_game,
        run_game_with_distant_turn, run_game_with_eye_comparison, run_game_with_horizontal_follow,
        run_game_with_lucid_perception, run_game_with_lucid_surface,
        run_game_with_solar_and_lucid_surface, run_game_with_solar_reveal,
        run_game_with_tentacle_auto_capture, run_game_with_tentacle_capture,
        run_game_with_tentacle_live_pursuit, run_game_with_tentacle_spatial_pursuit,
        run_game_with_void_eye, run_game_with_watching_stars,
    },
    visual::{
        oxygen_hud::OxygenHudOptions, static_creature::CreatureOptions,
        tentacle_live_pursuit::LivePursuitRequest, tentacle_solar_game::TentacleCaptureRequest,
    },
};

const SENTINEL: &[u8] = b"owned sentinel - never overwrite";
const HUD_ERROR: &str = "--oxygen-reserve requiere --oxygen-hud";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hud01_{name}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("existing_output.png"), SENTINEL).unwrap();
    dir
}

/// Opciones validas salvo, con `hud_enabled = false`, la reserva inicial sin `--oxygen-hud`.
/// `comparison`: etapas y mirada pedidas (lo que exigen las candidatas de la comparacion).
fn options(dir: &Path, hud_enabled: bool, comparison: bool) -> ConfiguredGameOptions {
    ConfiguredGameOptions {
        options: GameOptions {
            width: 320,
            height: 180,
            headless: true,
            frame_limit: None,
            output: dir.join("existing_output.png"),
            time: 0.0,
            near: None,
            culling: true,
            lod: true,
            overlay: false,
            creature: Some(CreatureOptions {
                model: dir.join("missing_model.obj"),
                texture: dir.join("missing_texture.png"),
                near: false,
            }),
            creature_reveal: false,
            creature_reveal_stages: comparison,
            creature_eye_follow: comparison,
            debug_oxygen: 1.0,
        },
        gameplay_feedback: false,
        oxygen_source: OxygenSource::Real,
        oxygen_seconds: None,
        presentation: false,
        oxygen_hud: OxygenHudOptions {
            enabled: hud_enabled,
            initial_reserve: Some(0.5),
        },
        start_menu: false,
    }
}

fn pursuit(dir: &Path) -> LivePursuitRequest {
    LivePursuitRequest {
        capture_dir: dir.join("missing_capture"),
        growth_package: dir.join("missing_growth.txt"),
        correspondence: dir.join("missing_correspondence.txt"),
    }
}

type Entry = Box<dyn Fn(ConfiguredGameOptions, &Path) -> Result<(), String>>;

fn entries() -> Vec<(&'static str, bool, Entry)> {
    let solar = PerceptionRoute {
        solar: true,
        ..PerceptionRoute::default()
    };
    vec![
        (
            "lucid_perception",
            false,
            Box::new(|o, _| run_game_with_lucid_perception(o, PerceptionRoute::default())),
        ),
        (
            "lucid_perception_solar",
            false,
            Box::new(move |o, _| run_game_with_lucid_perception(o, solar)),
        ),
        (
            "watching_stars",
            false,
            Box::new(|o, _| run_game_with_watching_stars(o, PerceptionRoute::default())),
        ),
        (
            "watching_stars_solar",
            false,
            Box::new(move |o, _| run_game_with_watching_stars(o, solar)),
        ),
        (
            "lucid_surface",
            false,
            Box::new(|o, _| run_game_with_lucid_surface(o)),
        ),
        (
            "void_eye_normal",
            false,
            Box::new(|o, _| run_game_with_void_eye(o, false, false)),
        ),
        (
            "void_eye_solar_lucid",
            false,
            Box::new(|o, _| run_game_with_void_eye(o, true, true)),
        ),
        (
            "eye_comparison",
            true,
            Box::new(|o, _| run_game_with_eye_comparison(o)),
        ),
        (
            "distant_turn",
            true,
            Box::new(|o, _| run_game_with_distant_turn(o)),
        ),
        (
            "horizontal_follow",
            true,
            Box::new(|o, _| run_game_with_horizontal_follow(o)),
        ),
        (
            "solar_reveal",
            false,
            Box::new(|o, _| run_game_with_solar_reveal(o)),
        ),
        (
            "solar_and_lucid_surface",
            false,
            Box::new(|o, _| run_game_with_solar_and_lucid_surface(o)),
        ),
        (
            "tentacle_capture",
            false,
            Box::new(move |o, d| {
                let p = pursuit(d);
                run_game_with_tentacle_capture(
                    o,
                    solar,
                    true,
                    false,
                    TentacleCaptureRequest {
                        capture_dir: p.capture_dir,
                        growth_package: p.growth_package,
                        correspondence: p.correspondence,
                        start_time: None,
                    },
                )
            }),
        ),
        (
            "tentacle_live_pursuit",
            false,
            Box::new(move |o, d| {
                run_game_with_tentacle_live_pursuit(o, solar, true, false, pursuit(d))
            }),
        ),
        (
            "tentacle_spatial_pursuit",
            false,
            Box::new(move |o, d| {
                run_game_with_tentacle_spatial_pursuit(o, solar, true, true, pursuit(d))
            }),
        ),
        (
            "tentacle_auto_capture",
            false,
            Box::new(move |o, d| {
                run_game_with_tentacle_auto_capture(o, solar, true, true, pursuit(d), None)
            }),
        ),
        (
            "tentacle_auto_capture_replay",
            false,
            Box::new(move |o, d| {
                let replay = d.join("replay_dir");
                let mut o = o;
                o.options.headless = false;
                run_game_with_tentacle_auto_capture(o, solar, true, false, pursuit(d), Some(replay))
            }),
        ),
    ]
}

#[test]
fn hud01_public_entries_reject_invalid_hud_before_any_path_io() {
    let mut failures = Vec::new();
    for (name, comparison, entry) in entries() {
        let dir = scratch(name);
        // Control: con el HUD valido la entrada llega a su IO de rutas (falla por el modelo inexistente u otra
        // entrada inexistente), no por el HUD: las demas opciones son validas hasta ese punto.
        let control = entry(options(&dir, true, comparison), &dir).unwrap_err();
        assert!(
            !control.contains("--oxygen"),
            "{name}: el control fallo por el HUD: {control}"
        );
        let error = entry(options(&dir, false, comparison), &dir).unwrap_err();
        assert_eq!(
            fs::read(dir.join("existing_output.png")).unwrap(),
            SENTINEL,
            "{name}: salida tocada"
        );
        if !error.contains(HUD_ERROR) {
            failures.push(format!("{name}: {error}"));
        }
    }
    assert!(
        failures.is_empty(),
        "HUD-01: rechazo del HUD despues del IO de rutas en:\n{}",
        failures.join("\n")
    );
}

#[test]
fn hud01_core_guard_still_rejects_for_the_plain_game() {
    // `run_game` no valida rutas antes del nucleo: la defensa central sigue rechazando antes de cargar nada.
    let dir = scratch("plain");
    let mut o = options(&dir, false, false);
    o.options.creature = None;
    let error = run_game(o).unwrap_err();
    assert!(error.contains(HUD_ERROR), "{error}");
    assert_eq!(fs::read(dir.join("existing_output.png")).unwrap(), SENTINEL);
}
