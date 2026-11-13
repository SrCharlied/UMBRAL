//! Un solo indicador de O2 en el juego (main_hud_01): el instrumento hundido de Persona 3 es la lectura de O2
//! y las notas de la presentacion de Persona 2 (aviso de recogida, mensaje de muerte por el monstruo y contadores)
//! quedan DEBAJO del hueco reservado para el ojo, sin otra barra ni otro porcentaje.

use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
    visual::{
        markers::draw_hud_notes,
        oxygen_hud::{OxygenReading, draw_oxygen_hud, oxygen_hud_layout},
    },
};

const NOTE_TEXT: u32 = 0xc9d6e0;
const TANK_CYAN: u32 = 0x3ff5df;
const DEATH_RED: u32 = 0xff4a3d;

fn presentation_world() -> World {
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    w.enable_sectors();
    w.oxygen_keys_drive_real = true;
    w
}

fn frame(w: &World, width: usize, height: usize) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    w.render(&mut fb);
    fb
}

/// Las notas para un estado: (antes, despues) del cuadro ya dibujado.
fn notes(w: &World, width: usize, height: usize) -> (Vec<u32>, Vec<u32>) {
    let mut fb = frame(w, width, height);
    let before = fb.pixels.clone();
    draw_hud_notes(&mut fb, w);
    (before, fb.pixels)
}

#[test]
fn main01_notes_never_cover_the_instrument_nor_the_eye_space() {
    for (width, height) in [(1280usize, 720usize), (320, 180)] {
        let l = oxygen_hud_layout(width, height).unwrap();
        let mut w = presentation_world();
        for (oxygen, flash, dying) in [
            (0.6, 0.0, false),
            (0.2, 0.0, false),
            (0.6, 1.0, false),
            (0.03, 0.0, true),
        ] {
            w.gameplay.oxygen01 = oxygen;
            w.pickup_flash = flash;
            w.last_pickup_gain = 0.25;
            w.dying = dying;
            for t in [0.0, 0.1, 0.2, 0.3] {
                w.time = t;
                let (before, after) = notes(&w, width, height);
                let limit = l.eye_reserve.y1 * width;
                assert!(
                    before[..limit] == after[..limit],
                    "{width}x{height} O2 {oxygen} flash {flash} dying {dying} t {t}: nota sobre el instrumento o el ojo"
                );
            }
        }
    }
}

#[test]
fn main02_notes_keep_the_pickup_notice_the_death_message_and_the_counters() {
    let mut w = presentation_world();
    w.gameplay.oxygen01 = 0.6;
    let (_, plain) = notes(&w, 1280, 720);
    assert!(
        plain.contains(&NOTE_TEXT),
        "contadores de registros y tanques"
    );
    w.pickup_flash = 1.0;
    w.last_pickup_gain = 0.25;
    let (_, flash) = notes(&w, 1280, 720);
    assert!(flash.contains(&TANK_CYAN), "aviso +25% O2");
    w.pickup_flash = 0.0;
    w.dying = true;
    w.time = 0.1;
    let (_, dying) = notes(&w, 1280, 720);
    assert!(dying.contains(&DEATH_RED), "SIN OXIGENO - ALGO TE ARRASTRA");
}

#[test]
fn main03_dying_reads_zero_even_while_the_monster_holds_the_reserve() {
    let mut w = presentation_world();
    w.gameplay.oxygen01 = 0.03;
    w.dying = true;
    let reading = OxygenReading::real_reserve(&w).unwrap();
    assert_eq!(
        reading.percent(),
        0,
        "durante la muerte por el monstruo el HUD marca 0 %"
    );
    let mut a = frame(&w, 320, 180);
    draw_oxygen_hud(&mut a, reading);
    let mut b = frame(&w, 320, 180);
    draw_oxygen_hud(&mut b, OxygenReading::new(0.0).unwrap());
    assert!(a.pixels == b.pixels);
    w.dying = false;
    assert_eq!(OxygenReading::real_reserve(&w).unwrap().percent(), 3);
}
