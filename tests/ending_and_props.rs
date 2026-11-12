//! Presentacion final (victoria/muerte) y props procedurales integrados en los planetas.

use umbral::{
    app::{PresentationLayers, compose_presentation},
    assets::{obj::load_model, ship_model_path},
    gameplay::GameplayState,
    math::Vec3,
    renderer::framebuffer::Framebuffer,
    scene::{
        solar_system::SolarSystem,
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_distant_turn::CreaturePreset,
        ending::{
            EndingClock, EndingKind, EndingSummary, FULLY_SHOWN, TEXT_DELAY, draw_ending,
            ending_text,
        },
        planet_props::{build_system_props, layers_for, prop_light_position},
    },
};

fn world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}

fn layers(ending: Option<f64>) -> PresentationLayers<'static> {
    PresentationLayers {
        overlay: None,
        comparison: false,
        distant_turn: false,
        horizontal: false,
        preset: CreaturePreset::Far,
        gameplay_feedback: false,
        perception: false,
        ending,
    }
}

fn mean_luma(fb: &Framebuffer) -> f64 {
    fb.pixels
        .iter()
        .map(|p| {
            let c = |s: u32| f64::from((p >> s) & 0xff);
            0.2126 * c(16) + 0.7152 * c(8) + 0.0722 * c(0)
        })
        .sum::<f64>()
        / fb.pixels.len() as f64
}

// ---------------------------------------------------------------- final

#[test]
fn el_tipo_de_final_sigue_al_gameplay_y_la_muerte_prevalece() {
    let mut g = GameplayState::default();
    assert_eq!(EndingKind::of(&g), None);
    g.objective_complete = true;
    assert_eq!(EndingKind::of(&g), Some(EndingKind::Victory));
    g.game_over = true;
    assert_eq!(EndingKind::of(&g), Some(EndingKind::Death));
}

#[test]
fn el_reloj_del_final_arranca_en_cero_avanza_acotado_y_se_reinicia() {
    let mut g = GameplayState::default();
    let mut clock = EndingClock::new();
    clock.update(&g, 0.5);
    assert_eq!(clock.seconds(), None, "sin final no hay reloj");
    g.game_over = true;
    clock.update(&g, 0.5);
    assert_eq!(
        clock.seconds(),
        Some(0.0),
        "el primer cuadro terminal es t = 0"
    );
    clock.update(&g, 0.1);
    clock.update(&g, f64::NAN);
    clock.update(&g, -3.0);
    assert!((clock.seconds().unwrap() - 0.1).abs() < 1e-12);
    clock.update(&g, 10.0);
    assert!(
        (clock.seconds().unwrap() - 0.35).abs() < 1e-12,
        "una pausa no salta la transicion"
    );
    // Shift+R: la partida nueva deja de ser terminal.
    let fresh = GameplayState::default();
    clock.update(&fresh, 0.1);
    assert_eq!(clock.seconds(), None);
}

#[test]
fn el_texto_refleja_registros_y_oxigeno() {
    let summary = |kind, logs, oxygen01| EndingSummary {
        kind,
        oxygen01,
        resources: 1,
        resources_total: 3,
        logs,
        logs_total: 3,
        mission_seconds: 125.4,
    };
    let (title, lines, stats) = ending_text(&summary(EndingKind::Death, 1, 0.0));
    assert_eq!(title, "SIN OXIGENO");
    assert!(lines.iter().any(|l| l.contains("2 registros")));
    assert!(stats.contains("02:05") && stats.contains("REGISTROS 1/3"));
    let (_, lines, _) = ending_text(&summary(EndingKind::Death, 2, 0.0));
    assert!(lines.iter().any(|l| l.starts_with("Un registro")));
    let (_, lines, _) = ending_text(&summary(EndingKind::Death, 3, 0.0));
    assert!(lines.iter().any(|l| l.contains("tres registros")));
    let (title, lines, _) = ending_text(&summary(EndingKind::Victory, 3, 0.274));
    assert_eq!(title, "EL UMBRAL");
    assert!(lines.iter().any(|l| l.contains("27%")));
    // Solo caracteres de la fuente 5x7: ASCII sin signos de apertura.
    for kind in [EndingKind::Death, EndingKind::Victory] {
        let (t, l, s) = ending_text(&summary(kind, 0, 0.5));
        for text in std::iter::once(t).chain(l).chain(std::iter::once(s)) {
            assert!(
                text.chars()
                    .all(|c| c.is_ascii_alphanumeric() || " .,:%/|-".contains(c)),
                "caracter fuera de la fuente en {text:?}"
            );
        }
    }
}

#[test]
fn la_transicion_oscurece_la_muerte_y_el_texto_aparece_despues() {
    let mut w = world();
    w.place_near(1, 20.0).unwrap();
    w.gameplay.game_over = true;
    w.gameplay.oxygen01 = 0.0;
    let mut scene = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    w.render(&mut scene);
    let base = mean_luma(&scene);
    let frame = |seconds: f64| {
        let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        w.render(&mut fb);
        compose_presentation(&mut fb, &w, &layers(Some(seconds))).unwrap();
        fb
    };
    let start = frame(0.0);
    assert_eq!(
        start.pixels, scene.pixels,
        "en t = 0 el cuadro es la escena intacta"
    );
    let dark = frame(TEXT_DELAY * 0.9);
    assert!(mean_luma(&dark) < base, "la muerte apaga la vista");
    let full = frame(FULLY_SHOWN);
    let text_color = 0xd8444a;
    assert!(
        full.pixels.contains(&text_color),
        "el titulo de muerte se dibuja completo"
    );
    assert!(
        !dark.pixels.contains(&text_color),
        "antes del retardo no hay texto"
    );
}

#[test]
fn la_victoria_pasa_por_un_destello_y_sin_final_no_cambia_nada() {
    let mut w = world();
    w.place_near(0, 30.0).unwrap();
    let mut plain = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    w.render(&mut plain);
    let reference = plain.pixels.clone();
    // Sin estado terminal, una capa de final pedida no dibuja nada.
    compose_presentation(&mut plain, &w, &layers(Some(5.0))).unwrap();
    assert_eq!(plain.pixels, reference);
    w.gameplay.collected_logs = 3;
    w.gameplay.objective_complete = true;
    let mut flash = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    w.render(&mut flash);
    compose_presentation(&mut flash, &w, &layers(Some(0.8))).unwrap();
    assert!(
        mean_luma(&flash) > mean_luma(&Framebuffer::new(320, 180, SPACE_COLOR).unwrap()) + 100.0
    );
}

#[test]
fn el_final_no_falla_con_framebuffers_minimos_ni_tiempos_raros() {
    let summary = EndingSummary {
        kind: EndingKind::Victory,
        oxygen01: 1.0,
        resources: 3,
        resources_total: 3,
        logs: 3,
        logs_total: 3,
        mission_seconds: f64::NAN,
    };
    for (w, h) in [(1, 1), (7, 3), (64, 36)] {
        let mut fb = Framebuffer::new(w, h, SPACE_COLOR).unwrap();
        for t in [0.0, 1.0, 5.0, FULLY_SHOWN, f64::INFINITY, f64::NAN, -1.0] {
            draw_ending(&mut fb, &summary, t);
        }
    }
}

// ---------------------------------------------------------------- props

#[test]
fn cada_planeta_tiene_su_contenido_y_el_sol_ninguno() {
    let system = SolarSystem::demo();
    let props = build_system_props(&system).unwrap();
    assert_eq!(props.len(), system.bodies.len());
    assert!(props[0].is_none(), "el Sol no lleva props");
    for (body, content) in system.bodies.iter().zip(&props).skip(1) {
        let content = content.as_ref().expect("cada planeta jugable tiene props");
        assert_eq!(content.body, body.name, "el contenido se busca por nombre");
        let layers = layers_for(body.name);
        assert_eq!(content.meshes.len(), layers.len());
        for (index, layer) in layers.iter().enumerate() {
            let count = content.props.iter().filter(|p| p.mesh == index).count();
            assert!(
                count >= 6 && count <= layer.max_count,
                "{} capa {index}: {count}",
                body.name
            );
        }
        for prop in &content.props {
            assert!((prop.normal.length() - 1.0).abs() < 1e-9);
            // El pie esta sobre la esfera unidad, hundido lo pedido.
            let foot = prop.anchor.local_matrix().transform_point(Vec3::ZERO);
            let elevation = layers[prop.mesh].elevation;
            assert!((foot - prop.normal * (1.0 + elevation)).length() < 1e-9);
        }
    }
}

#[test]
fn los_props_son_deterministas() {
    let a = build_system_props(&SolarSystem::demo()).unwrap();
    let b = build_system_props(&SolarSystem::demo()).unwrap();
    for (a, b) in a.iter().zip(&b) {
        match (a, b) {
            (Some(a), Some(b)) => assert_eq!(a.props, b.props),
            (None, None) => {}
            _ => panic!("contenido distinto entre construcciones"),
        }
    }
}

#[test]
fn los_props_no_alcanzan_las_balizas_ni_la_esfera_de_recoleccion() {
    let w = world();
    for anchor in w
        .gameplay
        .resources
        .iter()
        .chain(w.gameplay.logs.iter().map(|l| &l.anchor))
    {
        let body = &w.system.bodies[anchor.planet_index];
        let content = w.planet_props[anchor.planet_index].as_ref().unwrap();
        let top = body.body_radius * (1.0 + content.max_reach);
        let clearance = anchor.local_offset.length() - w.gameplay.settings.pickup_radius;
        assert!(
            top < clearance,
            "{}: props hasta {top:.2}, baliza desde {clearance:.2}",
            body.name
        );
    }
}

#[test]
fn la_luz_del_prop_es_la_real_de_dia_y_se_hunde_de_noche() {
    let sun = Vec3::ZERO;
    let center = Vec3::new(100.0, 0.0, 0.0);
    let day_normal = Vec3::new(-1.0, 0.0, 0.0);
    let day_base = center + day_normal * 10.0;
    assert_eq!(prop_light_position(sun, center, day_normal, day_base), sun);
    let night_normal = Vec3::new(1.0, 0.0, 0.0);
    let night_base = center + night_normal * 10.0;
    let light = prop_light_position(sun, center, night_normal, night_base);
    assert!((light - night_base).normalized().dot(night_normal) < -0.99);
    // Terminador: luz intermedia, finita.
    let dusk = Vec3::new(0.05, 1.0, 0.0).normalized();
    let light = prop_light_position(sun, center, dusk, center + dusk * 10.0);
    assert!(light.is_finite());
}

#[test]
fn los_props_se_dibujan_cerca_se_apagan_con_f2_y_no_cambian_los_objetos() {
    let mut w = world();
    w.place_near(1, 10.0).unwrap();
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    let with = w.render(&mut fb);
    let with_pixels = fb.pixels.clone();
    w.toggles.planet_props = false;
    let without = w.render(&mut fb);
    assert_eq!(with.objects_submitted, without.objects_submitted);
    assert_eq!(with.objects_culled, without.objects_culled);
    assert!(with.triangles_submitted > without.triangles_submitted);
    assert!(with.triangles_rasterized > without.triangles_rasterized);
    assert_ne!(with_pixels, fb.pixels, "los props se ven junto a Antena");
}

#[test]
fn reiniciar_la_partida_conserva_los_mismos_props() {
    let mut w = world();
    let before: Vec<_> = w
        .planet_props
        .iter()
        .map(|p| p.as_ref().map(|c| c.props.clone()))
        .collect();
    w.restart_game();
    let after: Vec<_> = w
        .planet_props
        .iter()
        .map(|p| p.as_ref().map(|c| c.props.clone()))
        .collect();
    assert_eq!(before, after);
}
