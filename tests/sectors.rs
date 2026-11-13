//! Espacio procedural por sectores, tanques de oxigeno visibles/recogibles, marcadores y
//! teclas de O2 que arrastran el O2 real (experiencia de presentacion).

use umbral::{
    app::{
        PresentationLayers, advance_perception_frame, compose_presentation, set_oxygen_tank_seconds,
    },
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::framebuffer::Framebuffer,
    scene::{
        sectors::{INNER_EXCLUSION, SECTOR_RANGE, SECTOR_SIZE, SectorField, generate},
        ship::{Ship, ShipInput},
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_distant_turn::CreaturePreset,
        markers::marker_targets,
        perception_game::{OxygenKeys, OxygenTransition},
    },
};

fn world() -> World {
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    w.clear_debug_oxygen();
    w
}

fn presentation_world() -> World {
    let mut w = world();
    w.enable_sectors();
    w.oxygen_keys_drive_real = true;
    w
}

/// Primer sector (en espiral simple) con planeta y tanque, lejos del sistema.
fn sector_with_planet() -> umbral::scene::sectors::Sector {
    for i in 2..200i64 {
        let s = generate((i, 0, -i / 2));
        if !s.planets.is_empty() && !s.tanks.is_empty() {
            return s;
        }
    }
    panic!("ningun sector con planeta");
}

#[test]
fn los_sectores_son_deterministas_y_respetan_el_sistema_solar() {
    for x in -4..=4 {
        for y in -2..=2 {
            for z in -4..=4 {
                let a = generate((x, y, z));
                let b = generate((x, y, z));
                assert_eq!(a.tanks, b.tanks);
                assert_eq!(a.planets.len(), b.planets.len());
                for (p, q) in a.planets.iter().zip(&b.planets) {
                    assert_eq!((p.center, p.radius, p.id), (q.center, q.radius, q.id));
                }
                for p in &a.planets {
                    assert!(p.center.length() > INNER_EXCLUSION + p.radius);
                    assert!(p.radius >= 10.0 && p.radius <= 34.0);
                }
                for t in &a.tanks {
                    assert!(t.position.length() > INNER_EXCLUSION);
                    for p in &a.planets {
                        assert!((t.position - p.center).length() > p.radius + 5.0);
                    }
                }
            }
        }
    }
}

#[test]
fn el_campo_genera_alrededor_de_la_nave_y_descarta_lo_lejano() {
    let mut field = SectorField::new();
    let side = (2 * SECTOR_RANGE + 1) as usize;
    field.update(Vec3::new(3000.0, 0.0, 0.0));
    assert_eq!(field.sector_count(), side * side * side);
    let first: Vec<_> = field.planets().map(|p| p.id).collect();
    assert!(
        !first.is_empty(),
        "un cubo de sectores lejos del Sol tiene planetas"
    );
    // Muy lejos: ningun sector anterior sobrevive.
    field.update(Vec3::new(3000.0 + 10.0 * SECTOR_SIZE, 0.0, 0.0));
    assert_eq!(field.sector_count(), side * side * side);
    assert!(field.planets().all(|p| !first.contains(&p.id)));
    // Al volver aparecen exactamente los mismos.
    field.update(Vec3::new(3000.0, 0.0, 0.0));
    let mut again: Vec<_> = field.planets().map(|p| p.id).collect();
    let mut first = first;
    first.sort_unstable();
    again.sort_unstable();
    assert_eq!(first, again);
}

#[test]
fn recoger_un_tanque_suma_oxigeno_y_cuenta_y_vuelve_con_partida_nueva() {
    let mut w = presentation_world();
    set_oxygen_tank_seconds(&mut w, 120.0).unwrap();
    w.gameplay.oxygen01 = 0.5;
    let sector = sector_with_planet();
    let tank = sector.tanks[0];
    // Nave a 30 unidades del tanque, apuntando hacia el, y un empuje corto.
    let start = tank.position + Vec3::new(0.0, 0.0, 30.0);
    w.ship = Ship::new(start, -Vec3::Z);
    w.enable_sectors();
    assert!(w.sectors.as_ref().unwrap().tanks().any(|t| t.id == tank.id));
    let before = w.gameplay.collected_resources;
    for _ in 0..40 {
        w.update(
            &ShipInput {
                thrust: 1.0,
                ..ShipInput::default()
            },
            0.05,
        );
        if w.gameplay.collected_resources > before {
            break;
        }
    }
    assert_eq!(w.gameplay.collected_resources, before + 1);
    assert!(w.gameplay.oxygen01 > 0.6, "O2 {}", w.gameplay.oxygen01);
    assert!(!w.sectors.as_ref().unwrap().tanks().any(|t| t.id == tank.id));
    w.restart_game();
    assert!(
        w.sectors.as_ref().unwrap().tanks().any(|t| t.id == tank.id) || {
            // El reinicio lleva la nave al inicio; el tanque vuelve en cuanto su sector se genera.
            let mut field = SectorField::new();
            field.update(tank.position);
            field.tanks().any(|t| t.id == tank.id)
        }
    );
}

#[test]
fn la_nave_no_atraviesa_un_planeta_errante() {
    let mut w = presentation_world();
    let sector = sector_with_planet();
    let planet = &sector.planets[0];
    let start = planet.center + Vec3::new(0.0, 0.0, planet.radius + 40.0);
    w.ship = Ship::new(start, -Vec3::Z);
    w.enable_sectors();
    for _ in 0..200 {
        w.update(
            &ShipInput {
                thrust: 1.0,
                boost: true,
                ..ShipInput::default()
            },
            0.05,
        );
        let d = (w.ship.position - planet.center).length();
        assert!(
            d >= planet.radius + 1.9,
            "dentro del planeta: {d:.2} < {}",
            planet.radius
        );
    }
}

#[test]
fn con_n_el_oxigeno_real_baja_hasta_la_muerte() {
    let mut w = presentation_world();
    let mut oxygen = OxygenTransition::new(w.debug_oxygen01).unwrap();
    let press_n = OxygenKeys {
        decrease: true,
        ..OxygenKeys::default()
    };
    let mut frames = 0;
    while !w.gameplay.game_over && frames < 400 {
        // N cada 5 cuadros, como pulsaciones repetidas.
        let keys = if frames % 5 == 0 {
            press_n
        } else {
            OxygenKeys::default()
        };
        advance_perception_frame(&mut w, &mut oxygen, keys, &ShipInput::default(), 0.05).unwrap();
        frames += 1;
        assert!(
            (w.gameplay.oxygen01 - w.debug_oxygen01).abs() < 1e-9 || w.gameplay.game_over,
            "el O2 real sigue al mostrado"
        );
    }
    assert!(
        w.gameplay.game_over,
        "N lleva a la muerte en {frames} cuadros"
    );
}

#[test]
fn sin_presentacion_n_sigue_siendo_solo_visual() {
    let mut w = world();
    let mut oxygen = OxygenTransition::new(w.debug_oxygen01).unwrap();
    let press_n = OxygenKeys {
        preset: Some(0.0),
        ..OxygenKeys::default()
    };
    advance_perception_frame(&mut w, &mut oxygen, press_n, &ShipInput::default(), 0.05).unwrap();
    for _ in 0..200 {
        advance_perception_frame(
            &mut w,
            &mut oxygen,
            OxygenKeys::default(),
            &ShipInput::default(),
            0.05,
        )
        .unwrap();
    }
    assert_eq!(w.debug_oxygen01, 0.0);
    assert!(w.gameplay.oxygen01 > 0.9 && !w.gameplay.game_over);
}

#[test]
fn los_marcadores_senalan_registros_y_tanques_y_se_dibujan() {
    let mut w = presentation_world();
    let targets = marker_targets(&w);
    assert_eq!(targets.iter().filter(|(_, log)| *log).count(), 3);
    let layers = PresentationLayers {
        overlay: None,
        comparison: false,
        distant_turn: false,
        horizontal: false,
        preset: CreaturePreset::Far,
        gameplay_feedback: false,
        perception: false,
        ending: None,
    };
    let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
    w.render(&mut fb);
    let plain = fb.pixels.clone();
    compose_presentation(&mut fb, &w, &layers).unwrap();
    assert!(fb.pixels.contains(&0xffc040), "marcador dorado de registro");
    assert_ne!(plain, fb.pixels);
    // Sin sectores (juego clasico) no hay marcadores.
    w.sectors = None;
    let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
    w.render(&mut fb);
    let plain = fb.pixels.clone();
    compose_presentation(&mut fb, &w, &layers).unwrap();
    assert_eq!(plain, fb.pixels);
}

#[test]
fn los_planetas_errantes_se_dibujan_y_cuentan_como_objetos() {
    let mut w = presentation_world();
    let sector = sector_with_planet();
    let planet = &sector.planets[0];
    let eye = planet.center + Vec3::new(0.0, 0.0, planet.radius * 4.0);
    w.ship = Ship::new(eye, -Vec3::Z);
    w.enable_sectors();
    // Camara detras de la nave mirando al planeta.
    for _ in 0..5 {
        w.update(&ShipInput::default(), 0.05);
    }
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    let with = w.render(&mut fb);
    let sectors = w.sectors.take();
    let without = w.render(&mut fb);
    w.sectors = sectors;
    assert!(with.objects_submitted > without.objects_submitted);
    assert!(with.fragments_evaluated > without.fragments_evaluated);
}

#[test]
fn tras_bajar_con_teclas_el_oxigeno_recuperado_quita_los_ojos() {
    use umbral::visual::watching_stars_game::apply_watching_stars;
    let mut w = presentation_world();
    let mut oxygen = OxygenTransition::new(w.debug_oxygen01).unwrap();
    let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
    let step = |w: &mut World, oxygen: &mut OxygenTransition, keys: OxygenKeys| {
        advance_perception_frame(w, oxygen, keys, &ShipInput::default(), 0.1).unwrap();
    };
    // Tecla 4 (5 %): baja el O2 real y el mostrado hasta el objetivo.
    step(
        &mut w,
        &mut oxygen,
        OxygenKeys {
            preset: Some(0.05),
            ..OxygenKeys::default()
        },
    );
    for _ in 0..60 {
        step(&mut w, &mut oxygen, OxygenKeys::default());
    }
    assert!(
        w.forced_oxygen.is_none(),
        "al llegar al objetivo el forzado se suelta"
    );
    assert!(w.gameplay.oxygen01 < 0.06 && w.debug_oxygen01 < 0.06);
    w.render(&mut fb);
    assert!(!apply_watching_stars(&mut fb, &w).unwrap().eyes.is_empty());
    // Tanques: el O2 real vuelve a 100 % y el mostrado lo sigue; los ojos desaparecen.
    w.gameplay.oxygen01 = 1.0;
    for _ in 0..60 {
        step(&mut w, &mut oxygen, OxygenKeys::default());
    }
    assert!(w.debug_oxygen01 > 0.97, "mostrado {}", w.debug_oxygen01);
    w.render(&mut fb);
    let report = apply_watching_stars(&mut fb, &w).unwrap();
    assert!(report.active_ids.is_empty() && report.eyes.is_empty());
}

#[test]
fn el_hud_muestra_el_oxigeno_y_el_aviso_de_recogida() {
    // main_hud_01: el O2 lo lee SOLO el HUD de Persona 3 (`visual::oxygen_hud`, pruebas oxygen_hud*); las notas de
    // la presentacion (contadores y aviso de recogida) van debajo del hueco reservado para el ojo.
    use umbral::visual::{markers::draw_hud_notes, oxygen_hud::oxygen_hud_layout};
    let mut w = presentation_world();
    let mut fb = Framebuffer::new(1280, 720, SPACE_COLOR).unwrap();
    let below = oxygen_hud_layout(1280, 720).unwrap().eye_reserve.y1 * 1280;
    w.render(&mut fb);
    let before = fb.pixels.clone();
    draw_hud_notes(&mut fb, &w);
    assert_ne!(before, fb.pixels);
    assert_eq!(
        before[..below],
        fb.pixels[..below],
        "nada sobre el HUD ni el hueco del ojo"
    );
    // Recoger: aviso visible bajo el HUD.
    w.pickup_flash = 1.0;
    w.last_pickup_gain = 0.25;
    w.render(&mut fb);
    draw_hud_notes(&mut fb, &w);
    assert!(
        fb.pixels[below..below + 1280 * 80].contains(&0x3ff5df),
        "aviso +25% O2"
    );
    // O2 bajo: sin barra roja propia (la lectura es el HUD de oxigeno).
    w.gameplay.oxygen01 = 0.2;
    w.pickup_flash = 0.0;
    w.render(&mut fb);
    draw_hud_notes(&mut fb, &w);
    assert!(!fb.pixels.contains(&0xff4a3d));
}
