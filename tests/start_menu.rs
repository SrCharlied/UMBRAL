//! Menu de inicio (start_menu_01, pedido de Charlie, al estilo DREDGE): titulo UMBRAL y botones sobre la escena
//! real del juego. Pruebas de la logica pura, del dibujo y de la CLI; la ventana la prueba Charlie.

use std::process::Command;
use umbral::{
    renderer::framebuffer::Framebuffer,
    visual::start_menu::{
        MENU_ITEMS, MenuAction, MenuInput, MenuScreen, SELECTED_TEXT, StartMenu, TITLE_COLOR,
        draw_start_menu, menu_layout,
    },
};

const UP: MenuInput = MenuInput {
    up: true,
    down: false,
    confirm: false,
    back: false,
};
const DOWN: MenuInput = MenuInput {
    up: false,
    down: true,
    confirm: false,
    back: false,
};
const OK: MenuInput = MenuInput {
    up: false,
    down: false,
    confirm: true,
    back: false,
};
const BACK: MenuInput = MenuInput {
    up: false,
    down: false,
    confirm: false,
    back: true,
};

#[test]
fn menu01_navigation_wraps_and_actions() {
    // Elegante como DREDGE (Charlie): solo UMBRAL, JUGAR y CONTROLES.
    assert_eq!(MENU_ITEMS.as_slice(), ["JUGAR", "CONTROLES"].as_slice());
    let mut m = StartMenu::new();
    assert_eq!(m.selected, 0);
    assert_eq!(m.step(UP), MenuAction::Stay);
    assert_eq!(m.selected, 1, "arriba desde el primero va al ultimo");
    assert_eq!(m.step(DOWN), MenuAction::Stay);
    assert_eq!(m.selected, 0, "abajo desde el ultimo vuelve al primero");
    assert_eq!(m.step(OK), MenuAction::StartGame, "JUGAR");
    assert_eq!(
        StartMenu::new().step(BACK),
        MenuAction::Quit,
        "Esc en el menu principal sale"
    );
}

#[test]
fn menu02_controls_screen_goes_back() {
    let mut m = StartMenu::new();
    m.step(DOWN);
    assert_eq!(m.step(OK), MenuAction::Stay);
    assert_eq!(m.screen, MenuScreen::Controls);
    assert_eq!(m.step(DOWN), MenuAction::Stay);
    assert_eq!(
        m.selected, 1,
        "en la pantalla de controles no se mueve la seleccion"
    );
    assert_eq!(m.step(BACK), MenuAction::Stay, "Esc vuelve, no sale");
    assert_eq!((m.screen, m.selected), (MenuScreen::Main, 1));
    m.step(OK);
    assert_eq!(m.step(OK), MenuAction::Stay, "Enter tambien vuelve");
    assert_eq!((m.screen, m.selected), (MenuScreen::Main, 1));
}

#[test]
fn menu03_layout_fits_left_side_without_overlaps() {
    for (w, h) in [
        (1280usize, 720usize),
        (1920, 1080),
        (640, 360),
        (320, 180),
        (800, 600),
    ] {
        let l = menu_layout(w, h);
        assert!(l.title_scale >= 3 && l.text_scale >= 1, "{w}x{h}: {l:?}");
        let rects = std::iter::once(l.title).chain(l.items).chain([l.panel]);
        for r in rects {
            assert!(
                r.w > 0 && r.h > 0 && r.x + r.w <= w && r.y + r.h <= h,
                "{w}x{h}: {r:?}"
            );
        }
        assert!(
            l.title.x + l.title.w <= w * 3 / 5,
            "{w}x{h}: titulo a la izquierda"
        );
        assert!(
            l.items[0].y >= l.title.y + l.title.h,
            "{w}x{h}: botones bajo el titulo"
        );
        for pair in l.items.windows(2) {
            assert!(
                pair[0].y + pair[0].h < pair[1].y,
                "{w}x{h}: botones encimados"
            );
        }
    }
}

#[test]
fn menu04_drawing_title_buttons_and_selection() {
    let draw = |m: &StartMenu, w: usize, h: usize| {
        let mut fb = Framebuffer::new(w, h, 0x405060).unwrap();
        draw_start_menu(&mut fb, m);
        fb
    };
    let base = Framebuffer::new(1280, 720, 0x405060).unwrap().pixels;
    let a = draw(&StartMenu::new(), 1280, 720);
    let l = menu_layout(1280, 720);
    let count = |fb: &Framebuffer, r: umbral::visual::start_menu::MenuRect, c: u32| {
        (r.y..r.y + r.h)
            .flat_map(|y| (r.x..r.x + r.w).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.pixels[y * 1280 + x] == c)
            .count()
    };
    assert!(
        count(&a, l.title, TITLE_COLOR) > 500,
        "titulo UMBRAL dibujado"
    );
    assert!(
        count(&a, l.items[0], SELECTED_TEXT) > 50,
        "boton elegido resaltado"
    );
    let mut second = StartMenu::new();
    second.selected = 1;
    let b = draw(&second, 1280, 720);
    assert_eq!(
        count(&b, l.items[0], SELECTED_TEXT),
        0,
        "el primero ya no esta elegido"
    );
    assert!(count(&b, l.items[1], SELECTED_TEXT) > 50);
    // Lado izquierdo oscurecido; el extremo derecho queda como la escena.
    let lum = |p: u32| ((p >> 16) & 0xff) + ((p >> 8) & 0xff) + (p & 0xff);
    assert!(
        lum(a.pixels[700 * 1280 + 5]) < lum(base[700 * 1280 + 5]),
        "degradado oscuro a la izquierda"
    );
    assert_eq!(
        a.pixels[360 * 1280 + 1275],
        base[360 * 1280 + 1275],
        "la escena sigue a la derecha"
    );
    // Submenus y tamanos diminutos sin panico.
    let controls = StartMenu {
        selected: 1,
        screen: MenuScreen::Controls,
    };
    assert!(
        draw(&controls, 1280, 720).pixels != a.pixels,
        "CONTROLES distinto del menu principal"
    );
    for (w, h) in [(1, 1), (40, 30), (100, 400), (320, 180)] {
        draw(&StartMenu::new(), w, h);
    }
}

#[test]
fn menu05_cli_knows_no_menu() {
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.contains("--no-menu") && text.contains("JUGAR"),
        "{text}"
    );
    let path = std::env::temp_dir().join(format!("start_menu_cli_{}.png", std::process::id()));
    let run = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--headless",
            "--no-menu",
            "--width",
            "64",
            "--height",
            "36",
            "--output",
        ])
        .arg(&path)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    std::fs::remove_file(path).unwrap();
}
