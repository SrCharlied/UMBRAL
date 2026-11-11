use super::*;
#[test]
#[ignore = "valid private fixtures, owned outputs"]
fn valid_fixtures_reach_replay_directory_and_completion_guards() {
    let d = scratch("real_guards");
    let call = |o, replay| {
        run_game_with_tentacle_auto_capture(o, solar_route(), true, true, private_request(), replay)
    };
    let mut control = options(d.join("positive.png"), private_creature());
    control.width = 64;
    control.height = 64;
    control.headless = true;
    call(control, None).expect("valid input graph positive control");
    let missing = d.join("missing");
    let error = call(
        options(d.join("p.png"), private_creature()),
        Some(missing.clone()),
    )
    .unwrap_err();
    assert!(
        error.contains("--auto-capture-replay") && error.contains("missing"),
        "{error}"
    );
    assert!(!missing.exists());
    let full = d.join("full");
    std::fs::create_dir(&full).unwrap();
    std::fs::write(full.join("sentinel"), b"ParentSentinel").unwrap();
    let error = call(
        options(d.join("p.png"), private_creature()),
        Some(full.clone()),
    )
    .unwrap_err();
    assert!(error.contains("carpeta VACIA"), "{error}");
    assert_eq!(
        std::fs::read(full.join("sentinel")).unwrap(),
        b"ParentSentinel"
    );
    let empty = d.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let mut limited = options(d.join("p.png"), private_creature());
    limited.frame_limit = Some(1);
    let error = call(limited, Some(empty.clone())).unwrap_err();
    assert!(error.contains("no se combina con --frames"), "{error}");
    assert_eq!(std::fs::read_dir(empty).unwrap().count(), 0);
    assert!(!d.join("p.png").exists());
}
#[test]
#[ignore = "valid private fixtures copied only into owned scratch"]
fn disposable_valid_graph_reaches_output_alias_and_hardlink_guards() {
    use umbral::assets::obj::load_model;
    let d = scratch("real_alias");
    let orig = env("UMBRAL_CAPTURE_CREATURE_OBJ");
    let model = d.join(orig.file_name().unwrap());
    std::fs::copy(&orig, &model).unwrap();
    for lib in load_model(&orig).unwrap().material_libraries {
        std::fs::copy(orig.parent().unwrap().join(&lib), d.join(lib)).unwrap();
    }
    let texture = d.join("texture.png");
    std::fs::copy(env("UMBRAL_CREATURE_TEXTURE"), &texture).unwrap();
    let before = std::fs::read(&texture).unwrap();
    let creature = || {
        Some(CreatureOptions {
            model: model.clone(),
            texture: texture.clone(),
            near: false,
        })
    };
    let call = |output| {
        let mut o = options(output, creature());
        o.width = 64;
        o.height = 64;
        o.headless = true;
        run_game_with_tentacle_auto_capture(o, solar_route(), true, true, private_request(), None)
    };
    call(d.join("positive.png")).expect("valid disposable fixture graph positive control");
    let error = call(texture.clone()).unwrap_err();
    assert!(error.contains("entrada"), "{error}");
    let alias = d.join("alias.png");
    std::fs::hard_link(&texture, &alias).unwrap();
    let error = call(alias.clone()).unwrap_err();
    assert!(
        error.contains("alias de entrada protegida (identidad de archivo)"),
        "{error}"
    );
    assert!(std::fs::read(&texture).unwrap() == before);
    assert!(std::fs::read(alias).unwrap() == before);
    println!("VALID_GRAPH alias_path=true hardlink_identity=true input_bytes_intact=true");
}

#[test]
#[ignore = "valid private fixtures, preserve non-auto legacy output policy"]
fn nonautomatic_guarded_routes_keep_legacy_output_behavior() {
    use umbral::app::{
        run_game_with_tentacle_live_pursuit, run_game_with_tentacle_spatial_pursuit,
    };
    let d = scratch("legacy_guarded");
    for (name, entry) in [
        (
            "live",
            run_game_with_tentacle_live_pursuit
                as fn(
                    GameOptions,
                    PerceptionRoute,
                    bool,
                    bool,
                    LivePursuitRequest,
                ) -> Result<(), String>,
        ),
        ("spatial", run_game_with_tentacle_spatial_pursuit),
    ] {
        let path = d.join(format!("{name}.png"));
        let call = || {
            let mut o = options(path.clone(), private_creature());
            o.width = 64;
            o.height = 64;
            o.headless = true;
            entry(o, solar_route(), true, true, private_request())
        };
        call().expect("valid legacy fresh-output control");
        let expected = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"owned legacy output").unwrap();
        call().expect("nonautomatic legacy output replacement must remain unchanged");
        assert_eq!(std::fs::read(path).unwrap(), expected);
    }
}
