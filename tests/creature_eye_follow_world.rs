//! Fase 2 de la mirada: flags, `World` y binario. Fixtures sinteticos (sin el asset).

use umbral::visual::creature_eye_follow::*;

// --- Flags (puro, antes de cualquier IO) ---

#[test]
fn eye_follow_flags_need_the_stages_and_the_creature_and_reject_legacy() {
    for stages in [false, true] {
        for creature in [false, true] {
            for legacy in [false, true] {
                assert_eq!(
                    validate_eye_follow_flags(false, stages, creature, legacy),
                    Ok(()),
                    "sin el flag nada cambia: {stages} {creature} {legacy}"
                );
            }
        }
    }
    assert_eq!(validate_eye_follow_flags(true, true, true, false), Ok(()));
    for (stages, creature, legacy, needle) in [
        (
            false,
            true,
            false,
            "--creature-eye-follow necesita --creature-reveal-stages",
        ),
        (
            false,
            false,
            false,
            "--creature-eye-follow necesita --creature-reveal-stages",
        ),
        (
            true,
            false,
            false,
            "--creature-eye-follow necesita --creature-model",
        ),
        (true, true, true, "--legacy no admite --creature-eye-follow"),
        (
            false,
            false,
            true,
            "--legacy no admite --creature-eye-follow",
        ),
    ] {
        let error = validate_eye_follow_flags(true, stages, creature, legacy)
            .expect_err(&format!("aceptado: {stages} {creature} {legacy}"));
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
}

// --- Fixtures del mundo ---

use umbral::{
    assets::{
        obj::{Model, load_model, parse_obj},
        ship_model_path,
    },
    math::Vec3,
    renderer::texture::Texture,
    scene::world::World,
    visual::{
        staged_creature_reveal::{EyeRegion, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const W: usize = 320;
const H: usize = 180;

/// Cupula de la fase 1 (radio 1, apice +Z, anillos de 10 grados, 16 segmentos): con
/// `dome_region` las bandas 0-4 son ojo y las 5-7 cuerpo; pivote en el origen y eje +Z.
fn dome_source() -> String {
    let segments = 16;
    let mut s = String::from("vt 0.5 0.5\nv 0 0 1\nvn 0 0 1\n");
    let mut index = vec![vec![1usize]];
    let mut next = 2;
    for k in 1..=8 {
        let phi = (k as f64 * 10.0).to_radians();
        let mut ring = Vec::new();
        for j in 0..segments {
            let t = (j as f64 / segments as f64) * std::f64::consts::TAU;
            let (x, y, z) = (phi.sin() * t.cos(), phi.sin() * t.sin(), phi.cos());
            s += &format!("v {x} {y} {z}\nvn {x} {y} {z}\n");
            ring.push(next);
            next += 1;
        }
        index.push(ring);
    }
    let f = |a: usize, b: usize, c: usize| format!("f {a}/1/{a} {b}/1/{b} {c}/1/{c}\n");
    for j in 0..segments {
        s += &f(1, index[1][j], index[1][(j + 1) % segments]);
    }
    for k in 1..8 {
        for j in 0..segments {
            let (a, b) = (index[k][j], index[k][(j + 1) % segments]);
            let (c, d) = (index[k + 1][j], index[k + 1][(j + 1) % segments]);
            s += &f(a, c, d);
            s += &f(a, d, b);
        }
    }
    s
}

/// Cupula sola: centro de bounds sobre el eje, asi que en el arranque `--near-creature` la nave
/// queda sobre el eje optico (mirada neutral), y su envolvente posada no cabe en la esfera.
fn dome() -> Model {
    parse_obj(&dome_source()).unwrap()
}

/// Cupula con un triangulo de cuerpo desplazado: bounds descentrados como en el asset real, la
/// envolvente posada cabe en la esfera original y en el arranque la mirada no es neutral.
fn offset_dome() -> Model {
    parse_obj(
        &(dome_source()
            + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n"),
    )
    .unwrap()
}

fn dome_region() -> EyeRegion {
    EyeRegion::new(Vec3::Z, 0.85).unwrap()
}

fn texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160],
    )
    .unwrap()
}

fn creature(model: Model) -> StaticCreature {
    StaticCreature::new(model, texture(), CreaturePlacement::demo()).unwrap()
}

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

/// Mundo con la criatura y la nave en el arranque `--near-creature`, sin modos.
fn creature_world(model: Model) -> World {
    let mut world = new_world();
    world.set_creature(creature(model)).unwrap();
    world.place_near_creature().unwrap();
    world
}

fn enable_stages(world: &mut World) {
    world
        .enable_staged_creature_reveal(StagedRevealSettings::default(), dome_region())
        .unwrap();
}

fn skin_of(model: &Model) -> EyeSkin {
    EyeSkin::calibrate(model, &dome_region()).unwrap()
}

// --- Ciclo de vida ---

#[test]
fn the_eye_follow_needs_the_stages_and_starts_snapped_toward_the_ship() {
    let mut world = creature_world(offset_dome());
    let error = world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .expect_err("sin etapas");
    assert!(error.contains("--creature-reveal-stages"), "{error}");
    assert!(world.creature_eye_follow().is_none());
    assert!(world.staged_creature_reveal().is_none());

    enable_stages(&mut world);
    world
        .enable_creature_eye_follow(EyeFollowSettings::new(0.8).unwrap())
        .unwrap();
    let follow = world.creature_eye_follow().expect("activa");
    assert_eq!(follow.settings().t95_seconds(), 0.8);
    let target = skin_of(&offset_dome()).aim(world.creature().unwrap(), world.ship.position);
    assert!(target.yaw() > 0.0 && target.pitch() < 0.0, "{target:?}");
    assert_eq!(follow.current(), target, "arranca ya en el objetivo");
}

/// Se parte con `dome_region` (2 caras de ojo y 1 de cuerpo) pero no calibra: sin costura.
fn seamless() -> Model {
    parse_obj(
        "v -0.1 -0.1 1\nv 0.1 -0.1 1\nv 0.1 0.1 1\nv -0.1 0.1 1\n\
         v 3 3 3\nv 4 3 3\nv 3 4 3\nf 1 2 3\nf 1 3 4\nf 5 6 7\n",
    )
    .unwrap()
}

#[test]
fn the_calibration_follows_the_creature_and_failed_setups_change_nothing() {
    let mut world = creature_world(offset_dome());
    let first = world.clear_creature().unwrap();
    enable_stages(&mut world);
    // Sin criatura se activa (como las etapas) y solo retiene el estado angular.
    world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .unwrap();
    assert!(world.creature_eye_skin().is_none());
    assert!(world.creature_eye_follow().unwrap().current().is_neutral());
    // Poner la criatura calibra una vez y salta al objetivo.
    world.set_creature(first).unwrap();
    assert_eq!(world.creature_eye_skin(), Some(&skin_of(&offset_dome())));
    let snapped = world.creature_eye_follow().unwrap();
    assert_eq!(
        snapped.current(),
        skin_of(&offset_dome()).aim(world.creature().unwrap(), world.ship.position)
    );
    assert!(!snapped.current().is_neutral());

    // Un modelo que se parte pero no calibra falla antes del cambio: todo sigue igual.
    let error = world
        .set_creature(creature(seamless()))
        .expect_err("no calibra");
    assert!(error.contains("costura"), "{error}");
    assert_eq!(world.creature().unwrap().model().faces.len(), 241);
    assert_eq!(world.staged_creature_parts().unwrap().eye_faces(), 144);
    assert_eq!(world.creature_eye_skin(), Some(&skin_of(&offset_dome())));
    assert_eq!(world.creature_eye_follow(), Some(snapped));

    // Otro asset: calibracion nueva (sin skin viejo) y salto a su objetivo.
    world.set_creature(creature(dome())).unwrap();
    assert_eq!(world.creature_eye_skin(), Some(&skin_of(&dome())));
    assert_eq!(
        world.creature_eye_follow().unwrap().current(),
        skin_of(&dome()).aim(world.creature().unwrap(), world.ship.position)
    );

    // Quitar la criatura suelta la calibracion y la cache; el modo y el estado siguen.
    world.clear_creature().unwrap();
    assert!(world.creature_eye_skin().is_none());
    assert!(world.creature_eye_follow().is_some());

    // Activar con una criatura que no calibra: `Err` y la mirada sigue como estaba.
    world.disable_staged_creature_reveal();
    world.set_creature(creature(seamless())).unwrap();
    enable_stages(&mut world);
    let error = world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .expect_err("no calibra");
    assert!(error.contains("costura"), "{error}");
    assert!(world.creature_eye_follow().is_none());
    assert!(world.staged_creature_reveal().is_some());
}

use umbral::{
    renderer::{framebuffer::Framebuffer, stats::RenderStats},
    scene::world::SPACE_COLOR,
    visual::creature_reveal::RevealSettings,
};

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

#[test]
fn mode_changes_drop_the_gaze_and_disabling_it_keeps_the_stages_and_the_oxygen() {
    let mut world = creature_world(offset_dome());
    enable_stages(&mut world);
    world.debug_oxygen01 = 0.25;
    world.snap_creature_reveal();
    let staged = render(&world);
    let eye = || EyeFollowSettings::default();

    world.enable_creature_eye_follow(eye()).unwrap();
    world.disable_creature_eye_follow();
    assert!(world.creature_eye_follow().is_none() && world.creature_eye_skin().is_none());
    assert_eq!(world.debug_oxygen01, 0.25);
    assert_eq!(
        world.staged_creature_reveal().unwrap().alphas(),
        (1.0, 0.02)
    );
    assert_eq!(render(&world), staged, "etapas originales exactas");

    // Activarla otra vez sustituye los ajustes.
    world.enable_creature_eye_follow(eye()).unwrap();
    world
        .enable_creature_eye_follow(EyeFollowSettings::new(3.0).unwrap())
        .unwrap();
    assert_eq!(
        world
            .creature_eye_follow()
            .unwrap()
            .settings()
            .t95_seconds(),
        3.0
    );
    // V1 sustituye a las etapas y con ellas a la mirada.
    world.enable_creature_reveal(RevealSettings::default());
    assert!(world.creature_eye_follow().is_none());
    // Desactivar las etapas la suelta.
    world.disable_creature_reveal();
    enable_stages(&mut world);
    world.enable_creature_eye_follow(eye()).unwrap();
    world.disable_staged_creature_reveal();
    assert!(world.creature_eye_follow().is_none());
    // Reactivar las etapas (con la misma u otra region) la reinicia apagada: sin skin viejo.
    enable_stages(&mut world);
    world.enable_creature_eye_follow(eye()).unwrap();
    enable_stages(&mut world);
    assert!(world.creature_eye_follow().is_none() && world.creature_eye_skin().is_none());
    assert_eq!(world.debug_oxygen01, 0.25);
}

// --- Estado temporal ---

use umbral::scene::ship::ShipInput;

fn gaze_world() -> World {
    let mut world = creature_world(offset_dome());
    enable_stages(&mut world);
    world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .unwrap();
    world
}

#[test]
fn the_gaze_eases_toward_the_ship_with_real_time_only() {
    let mut world = gaze_world();
    let skin = skin_of(&offset_dome());
    let start = world.creature_eye_follow().unwrap().current();
    // Ni la fisica ni F5 mueven la mirada, aunque la nave cambie de sitio.
    world.toggles.freeze_orbits = true;
    world.ship.position += Vec3::new(60.0, 25.0, 0.0);
    world.update(&ShipInput::default(), 0.1);
    assert_eq!(world.creature_eye_follow().unwrap().current(), start);
    let time = world.time;
    // Con segundos reales persigue a la nave (t95 = 1.5 s), no a la camara.
    let creature = world.creature().unwrap();
    let target = skin.aim(creature, world.ship.position);
    let camera = skin.aim(creature, world.camera.position);
    assert!(
        target != start && target != camera,
        "{start:?} {target:?} {camera:?}"
    );
    let mut expected = EyeFollow::new(EyeFollowSettings::default());
    expected.snap(start);
    expected.update(target, 1.5);
    world.update_creature_eye_follow(1.5);
    let current = world.creature_eye_follow().unwrap().current();
    assert_eq!(current, expected.current());
    let progress = (current.yaw() - start.yaw()) / (target.yaw() - start.yaw());
    assert!((progress - 0.95).abs() < 1e-9, "{progress}");
    assert_eq!(world.time, time, "no suma al tiempo orbital");
    // Un cuadro largo (sin el limite de la fisica) llega al objetivo.
    world.update_creature_eye_follow(60.0);
    assert_eq!(world.creature_eye_follow().unwrap().current(), target);
}

#[test]
fn resets_keep_the_oxygen_and_snap_the_gaze_to_the_relocated_ship() {
    let mut world = gaze_world();
    let skin = skin_of(&offset_dome());
    let aimed = |world: &World| skin.aim(world.creature().unwrap(), world.ship.position);
    type Reset = fn(&mut World);
    let resets: [(&str, Reset); 3] = [
        ("reset_ship", |w| w.reset_ship()),
        ("place_near", |w| w.place_near(1, 25.0).unwrap()),
        ("place_near_creature", |w| w.place_near_creature().unwrap()),
    ];
    for (name, reset) in resets {
        // Mirada a medio camino de otro objetivo antes del reinicio.
        world.ship.position += Vec3::new(-70.0, 30.0, 0.0);
        world.update_creature_eye_follow(0.2);
        world.debug_oxygen01 = 0.3;
        reset(&mut world);
        assert_eq!(world.debug_oxygen01, 0.3, "{name}");
        assert_eq!(
            world.creature_eye_follow().unwrap().current(),
            aimed(&world),
            "{name}"
        );
    }
}

// --- Render ---

use umbral::visual::{
    creature_reveal::{RevealCompositor, RevealPart},
    staged_creature_reveal::StagedParts,
};

/// Un triangulo degenerado: oculta la nave sin tocar su camara.
fn no_ship() -> Model {
    parse_obj("v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3\n").unwrap()
}

/// Fija el O2 y salta al objetivo de las etapas (la mirada no cambia).
fn at_oxygen(world: &mut World, o2: f64) -> (Vec<u32>, RenderStats) {
    world.debug_oxygen01 = o2;
    world.snap_creature_reveal();
    render(world)
}

/// Referencia con la API publica: el cuadro sin criatura y encima, con `alphas`, el ojo de la
/// fase 1 posado en `angles` y el cuerpo de reposo de la particion de las etapas (la nave del
/// mundo debe ser invisible). Devuelve tambien los contadores de las dos partes.
fn posed_reference(
    world: &mut World,
    angles: GazeAngles,
    (eye_alpha, body_alpha): (f64, f64),
) -> (Vec<u32>, RenderStats, RenderStats) {
    let follow = world.creature_eye_follow();
    let creature = world.clear_creature().unwrap();
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let base = world.render(&mut fb);
    let model = creature.model();
    let skin = skin_of(model);
    let mut cache = EyePoseCache::new(model, &skin).unwrap();
    cache.pose(model, &skin, angles).unwrap();
    let parts = StagedParts::split(model, &dome_region()).unwrap();
    let transform = creature.transform();
    let part = |model| RevealPart {
        model,
        transform: &transform,
        texture: creature.texture(),
    };
    let light = world.system.light_position();
    let mut compositor = RevealCompositor::new();
    let mut drawn = compositor
        .render_part(
            &mut fb,
            part(cache.eye_model()),
            &world.camera,
            light,
            eye_alpha,
        )
        .unwrap();
    if body_alpha > 0.0 {
        drawn += compositor
            .render_part(
                &mut fb,
                part(parts.body()),
                &world.camera,
                light,
                body_alpha,
            )
            .unwrap();
    }
    world.set_creature(creature).unwrap();
    // `set_creature` vuelve a saltar al objetivo; se restaura el estado anterior.
    assert_eq!(world.creature_eye_follow(), follow);
    (fb.pixels, base, drawn)
}

#[test]
fn the_full_creature_draws_the_posed_eye_and_the_bind_body_before_the_ship() {
    let mut world = gaze_world();
    world.ship_model = no_ship();
    let (frame, stats) = at_oxygen(&mut world, 0.05);
    assert_eq!(world.staged_creature_reveal().unwrap().alphas(), (1.0, 1.0));
    let angles = world.creature_eye_follow().unwrap().current();
    assert!(!angles.is_neutral());
    let (reference, base, drawn) = posed_reference(&mut world, angles, (1.0, 1.0));
    assert_eq!(frame, reference);
    // Una criatura logica; el costo real de las dos partes (los vertices dos veces).
    let vertices = world.creature().unwrap().model().vertices.len();
    assert_eq!(stats.objects_submitted, base.objects_submitted + 1);
    assert_eq!(
        stats.vertices_processed,
        base.vertices_processed + 2 * vertices
    );
    assert_eq!(stats.triangles_submitted, base.triangles_submitted + 241);
    let mut expected = base;
    expected += drawn;
    expected.objects_submitted = base.objects_submitted + 1;
    assert_eq!(stats, expected);
    // El ojo se movio: no es la estatica.
    world.disable_staged_creature_reveal();
    assert_ne!(render(&world).0, frame);
}

/// Pone la nave sobre el eje optico del ojo y deja que la mirada llegue (neutral exacta).
fn ship_on_the_optical_axis(world: &mut World, model: &Model) {
    let skin = skin_of(model);
    let creature = world.creature().unwrap();
    let pivot = creature.transform().matrix().transform_point(skin.pivot());
    world.ship.position = pivot + skin.optical_axis() * 200.0;
    world.update_creature_eye_follow(60.0);
    assert!(world.creature_eye_follow().unwrap().current().is_neutral());
}

#[test]
fn a_neutral_gaze_inside_the_bind_sphere_keeps_the_staged_paths_exactly() {
    let mut world = gaze_world();
    world.ship_model = no_ship();
    ship_on_the_optical_axis(&mut world, &offset_dome());
    assert!(world.creature_eye_skin().unwrap().fits_bind_sphere());
    for o2 in [0.05, 0.15, 0.25, 0.40, 1.0] {
        let with_eye = at_oxygen(&mut world, o2);
        world.disable_creature_eye_follow();
        let staged = at_oxygen(&mut world, o2);
        assert_eq!(with_eye, staged, "O2 {o2}");
        world
            .enable_creature_eye_follow(EyeFollowSettings::default())
            .unwrap();
        world.update_creature_eye_follow(60.0);
    }
}

#[test]
fn an_expanded_envelope_never_pretends_to_be_the_bind_sphere_even_when_neutral() {
    // Cupula sola: en el arranque la nave queda sobre el eje (neutral) pero la envolvente posada
    // no cabe en la esfera original; se dibuja por la ruta del ojo, con su propia envolvente.
    let mut world = creature_world(dome());
    world.ship_model = no_ship();
    enable_stages(&mut world);
    world
        .enable_creature_eye_follow(EyeFollowSettings::default())
        .unwrap();
    assert!(!world.creature_eye_skin().unwrap().fits_bind_sphere());
    let angles = world.creature_eye_follow().unwrap().current();
    assert!(angles.is_neutral());
    let (frame, stats) = at_oxygen(&mut world, 0.05);
    let (reference, base, drawn) = posed_reference(&mut world, angles, (1.0, 1.0));
    assert_eq!(frame, reference);
    let mut expected = base;
    expected += drawn;
    expected.objects_submitted = base.objects_submitted + 1;
    assert_eq!(stats, expected);
}

#[test]
fn partial_stages_ghost_the_posed_eye_then_the_bind_body_last() {
    let mut world = gaze_world();
    world.ship_model = no_ship();
    let angles = world.creature_eye_follow().unwrap().current();
    assert!(!angles.is_neutral());
    let vertices = world.creature().unwrap().model().vertices.len();
    for (o2, parts) in [(0.40, 1), (0.30, 1), (0.25, 2), (0.15, 2)] {
        let (frame, stats) = at_oxygen(&mut world, o2);
        let alphas = world.staged_creature_reveal().unwrap().alphas();
        assert!(alphas.1 < 1.0, "O2 {o2}: parcial");
        let (reference, base, drawn) = posed_reference(&mut world, angles, alphas);
        assert_eq!(frame, reference, "O2 {o2}");
        let mut expected = base;
        expected += drawn;
        expected.objects_submitted = base.objects_submitted + 1;
        assert_eq!(stats, expected, "O2 {o2}");
        assert_eq!(
            stats.vertices_processed,
            base.vertices_processed + parts * vertices,
            "O2 {o2}"
        );
    }
}

/// Mundo del arranque normal (sin nave visible) con la cupula sola en `center`, las etapas al
/// 5 % y, si `eye`, la mirada.
fn placed_dome_world(center: Vec3, radius: f64, culling: bool, eye: bool) -> World {
    let mut world = new_world();
    world.ship_model = no_ship();
    world.toggles.frustum_culling = culling;
    let placement = CreaturePlacement::new(center, radius, 0.0).unwrap();
    world
        .set_creature(StaticCreature::new(dome(), texture(), placement).unwrap())
        .unwrap();
    enable_stages(&mut world);
    world.debug_oxygen01 = 0.05;
    world.snap_creature_reveal();
    if eye {
        world
            .enable_creature_eye_follow(EyeFollowSettings::default())
            .unwrap();
    }
    world
}

fn base_world(culling: bool) -> (Vec<u32>, RenderStats) {
    let mut world = new_world();
    world.ship_model = no_ship();
    world.toggles.frustum_culling = culling;
    render(&world)
}

#[test]
fn culling_uses_the_posed_envelope_and_never_the_insufficient_bind_sphere() {
    // Justo fuera del frustum del arranque normal: la esfera original queda fuera y la
    // envolvente posada de la cupula (mayor) lo toca.
    let camera = new_world().camera;
    let forward = (camera.target - camera.position).normalized();
    let right = forward.cross(camera.up).normalized();
    let frustum = camera.frustum();
    let radius = 30.0;
    let ratio = skin_of(&dome()).posed_bounds_radius() / dome().bounds_radius;
    let center = (1..4000)
        .map(|k| camera.position + forward * 300.0 + right * (k as f64 * 0.5))
        .find(|&c| !frustum.sphere_visible(c, radius))
        .unwrap();
    assert!(
        frustum.sphere_visible(center, radius * ratio),
        "precondicion"
    );
    let vertices = dome().vertices.len();
    for culling in [true, false] {
        let base = base_world(culling).1;
        let stat = render(&placed_dome_world(center, radius, culling, false)).1;
        assert_eq!(
            stat.objects_culled,
            base.objects_culled + usize::from(culling)
        );
        let (_, eye) = render(&placed_dome_world(center, radius, culling, true));
        assert_eq!(eye.objects_culled, base.objects_culled, "culling {culling}");
        assert_eq!(
            eye.vertices_processed,
            base.vertices_processed + 2 * vertices
        );
    }
    // Detras de la camara: con descarte solo se cuenta (caras completas, 0 vertices).
    let behind = camera.position - forward * 500.0;
    let (base_frame, base) = base_world(true);
    let (frame, stats) = render(&placed_dome_world(behind, radius, true, true));
    let mut expected = base;
    expected += RenderStats {
        objects_submitted: 1,
        objects_culled: 1,
        triangles_submitted: 240,
        triangles_culled: 240,
        ..RenderStats::default()
    };
    assert_eq!((frame, stats), (base_frame, expected));
    let (_, off) = render(&placed_dome_world(behind, radius, false, true));
    assert_eq!(off.objects_culled, base_world(false).1.objects_culled);
    assert_eq!(
        off.vertices_processed,
        base_world(false).1.vertices_processed + 2 * vertices
    );
}

#[test]
fn the_eye_is_posed_lazily_once_per_angle_and_never_while_hidden_or_culled() {
    let mut world = gaze_world();
    world.ship_model = no_ship();
    let bytes = world.creature_eye_cache_bytes();
    assert!(bytes > 0);
    assert_eq!(world.creature_eye_pose_updates(), Some(0));
    // Oculta (100 %): los angulos avanzan, pero no se posa nada y es el cuadro sin criatura.
    let moved = Vec3::new(40.0, -20.0, 0.0);
    world.debug_oxygen01 = 1.0;
    world.snap_creature_reveal();
    world.ship.position += moved;
    let before = world.creature_eye_follow().unwrap().current();
    world.update_creature_eye_follow(0.3);
    assert_ne!(world.creature_eye_follow().unwrap().current(), before);
    let mut empty = gaze_world();
    empty.ship_model = no_ship();
    empty.clear_creature();
    empty.ship.position += moved;
    assert_eq!(render(&world), render(&empty));
    assert_eq!(world.creature_eye_pose_updates(), Some(0));
    assert_eq!(world.creature_eye_posed(), Some(GazeAngles::NEUTRAL));

    // Visible: posa una vez; repetir el cuadro (u otra resolucion) no vuelve a posar.
    let first = at_oxygen(&mut world, 0.05);
    let current = world.creature_eye_follow().unwrap().current();
    assert_eq!(world.creature_eye_posed(), Some(current));
    assert_eq!(world.creature_eye_pose_updates(), Some(1));
    for _ in 0..3 {
        assert_eq!(render(&world), first);
    }
    let mut small = Framebuffer::new(160, 90, SPACE_COLOR).unwrap();
    let _ = world.render(&mut small);
    assert_eq!(render(&world), first);
    assert_eq!(world.creature_eye_pose_updates(), Some(1));
    // Otro angulo: una pose mas, en sitio (la memoria no crece).
    world.ship.position += moved;
    world.update_creature_eye_follow(0.2);
    let _ = render(&world);
    assert_eq!(world.creature_eye_pose_updates(), Some(2));
    assert_eq!(world.creature_eye_cache_bytes(), bytes);

    // Descartada (camara mirando al otro lado): cambia el angulo y no se posa; sin descarte,
    // si (paga la etapa de vertices).
    let c = world.camera;
    world.camera.target = c.position + (c.position - c.target);
    world.update_creature_eye_follow(0.2);
    let (_, culled) = render(&world);
    assert_eq!(culled.objects_culled, base_world(true).1.objects_culled + 1);
    assert_eq!(world.creature_eye_pose_updates(), Some(2));
    world.toggles.frustum_culling = false;
    let _ = render(&world);
    assert_eq!(world.creature_eye_pose_updates(), Some(3));
    assert_eq!(
        world.creature_eye_posed(),
        Some(world.creature_eye_follow().unwrap().current())
    );
    // Sin criatura: sin cache.
    world.clear_creature();
    assert_eq!(
        (
            world.creature_eye_cache_bytes(),
            world.creature_eye_pose_updates()
        ),
        (0, None)
    );
}

/// Cuadro sin criatura con la nave actual del mundo.
fn without_creature(world: &mut World) -> (Vec<u32>, RenderStats) {
    let follow = world.creature_eye_follow();
    let creature = world.clear_creature().unwrap();
    let frame = render(world);
    world.set_creature(creature).unwrap();
    assert_eq!(world.creature_eye_follow(), follow);
    frame
}

#[test]
fn with_true_depth_the_full_creature_goes_before_the_ship_and_the_partial_after_it() {
    let mut world = gaze_world();
    // Nave y camara 30 mas arriba: la nave se ve igual y la criatura baja hasta quedar detras.
    let up = Vec3::Y * 30.0;
    world.ship.position += up;
    world.camera.position += up;
    world.camera.target += up;
    world.update_creature_eye_follow(60.0);
    let angles = world.creature_eye_follow().unwrap().current();
    assert!(!angles.is_neutral());
    let (full, full_stats) = at_oxygen(&mut world, 0.05);
    let (base_frame, base) = without_creature(&mut world);
    // La criatura posada sin nave, y el fondo sin nave ni criatura.
    let ship = std::mem::replace(&mut world.ship_model, no_ship());
    let (alone, _, drawn) = posed_reference(&mut world, angles, (1.0, 1.0));
    let (empty, _) = without_creature(&mut world);
    world.ship_model = ship;
    let creature_px: Vec<usize> = (0..W * H).filter(|&i| alone[i] != empty[i]).collect();
    let ship_px: Vec<usize> = (0..W * H).filter(|&i| base_frame[i] != empty[i]).collect();
    let overlap: Vec<usize> = ship_px
        .iter()
        .copied()
        .filter(|i| creature_px.contains(i))
        .collect();
    let bbox = |px: &[usize]| {
        px.iter().fold((W, H, 0, 0), |(a, b, c, d), &i| {
            (a.min(i % W), b.min(i / W), c.max(i % W), d.max(i / W))
        })
    };
    assert!(
        overlap.len() > 50,
        "la nave debe tapar parte de la criatura: criatura {} {:?}, nave {} {:?}, solape {}",
        creature_px.len(),
        bbox(&creature_px),
        ship_px.len(),
        bbox(&ship_px),
        overlap.len()
    );

    // Entera: la nave (delante) queda intacta y el resto es la criatura posada.
    for i in 0..W * H {
        let expected = if ship_px.contains(&i) {
            base_frame[i]
        } else {
            alone[i]
        };
        assert_eq!(full[i], expected, "pixel {i}");
    }
    // Antes de la nave: la criatura evalua tambien los fragmentos que la nave tapa despues.
    let mut expected = base;
    expected += drawn;
    expected.objects_submitted = base.objects_submitted + 1;
    assert_eq!(full_stats, expected);

    // Parcial: fantasma de una capa despues de la nave; donde la nave esta delante, intacta.
    let (partial, partial_stats) = at_oxygen(&mut world, 0.25);
    let alphas = world.staged_creature_reveal().unwrap().alphas();
    let (reference, base, drawn) = posed_reference(&mut world, angles, alphas);
    assert_eq!(partial, reference);
    assert!(overlap.iter().all(|&i| partial[i] == base_frame[i]));
    let mut expected = base;
    expected += drawn;
    expected.objects_submitted = base.objects_submitted + 1;
    assert_eq!(partial_stats, expected);
    assert!(
        partial_stats.fragments_evaluated < full_stats.fragments_evaluated,
        "despues de la nave el z-test rechaza lo tapado"
    );
}

// --- Linea de comandos ---

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::visual::staged_creature_reveal::CREATURE_EYE_REGION;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-eye-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_texture(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160])
        .unwrap();
    writer.finish().unwrap();
}

fn offset_dome_source() -> String {
    dome_source() + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n"
}

/// `offset_dome` escalada y movida para que su apice caiga en el centro de la ROI calibrada
/// del asset real (la que usa la linea de comandos): `dome_region` pasa a ser esa ROI.
fn calibrated_source() -> String {
    let (c, s) = (
        CREATURE_EYE_REGION.center(),
        CREATURE_EYE_REGION.radius() / 0.85,
    );
    let mut out = String::new();
    for line in offset_dome_source().lines() {
        match line.strip_prefix("v ") {
            Some(rest) => {
                let p: Vec<f64> = rest.split(' ').map(|x| x.parse().unwrap()).collect();
                let q = c + (Vec3::new(p[0], p[1], p[2]) - Vec3::Z) * s;
                out += &format!("v {} {} {}\n", q.x, q.y, q.z);
            }
            None => out += &format!("{line}\n"),
        }
    }
    out
}

/// Se parte con la ROI calibrada pero no calibra: el ojo no comparte vertices con el cuerpo.
fn seamless_source() -> String {
    let c = CREATURE_EYE_REGION.center();
    let v = |x: f64, y: f64, z: f64| format!("v {} {} {}\n", c.x + x, c.y + y, c.z + z);
    v(-0.05, -0.05, 0.0)
        + &v(0.05, -0.05, 0.0)
        + &v(0.05, 0.05, 0.0)
        + &v(-0.05, 0.05, 0.0)
        + &v(1.0, 1.0, -1.0)
        + &v(1.2, 1.0, -1.0)
        + &v(1.0, 1.2, -1.0)
        + "f 1 2 3\nf 1 3 4\nf 5 6 7\n"
}

/// Siempre con `--headless`: estas pruebas nunca abren ventana.
fn run_game(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_game_command_line_wires_the_eye_follow_and_validates_before_any_io() {
    let dir = temp_dir("cli");
    let (model, texture_path) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, calibrated_source()).unwrap();
    write_texture(&texture_path);
    let seamless = dir.join("sin-costura.obj");
    std::fs::write(&seamless, seamless_source()).unwrap();
    let texture_bytes = std::fs::read(&texture_path).unwrap();
    let output = dir.join("salida.png");
    let os = OsStr::new;
    let outcome = std::panic::catch_unwind(|| {
        let creature = [
            os("--creature-model"),
            model.as_os_str(),
            os("--creature-texture"),
            texture_path.as_os_str(),
        ];
        // Rutas inexistentes: cada error es de validacion, no de IO.
        let missing = [
            os("--creature-model"),
            os("no-existe.obj"),
            os("--creature-texture"),
            os("no-existe.png"),
        ];
        let eye = os("--creature-eye-follow");
        let stages = os("--creature-reveal-stages");
        let needs_stages = "--creature-eye-follow necesita --creature-reveal-stages";
        let failing: Vec<(Vec<&OsStr>, &str)> = vec![
            (vec![eye], needs_stages),
            ([&missing[..], &[eye]].concat(), needs_stages),
            (
                [&missing[..], &[os("--creature-reveal"), eye]].concat(),
                needs_stages,
            ),
            (
                vec![os("--legacy"), eye],
                "--legacy no admite --creature-eye-follow",
            ),
            (vec![stages, eye], "--creature-reveal-stages necesita"),
            // Con los dos modos de revelacion se conserva el error heredado.
            (
                [&missing[..], &[os("--creature-reveal"), stages, eye]].concat(),
                "incompatibles",
            ),
            // Se parte con la ROI pero no calibra: falla cerrado al cargar, sin PNG.
            (
                vec![
                    os("--creature-model"),
                    seamless.as_os_str(),
                    os("--creature-texture"),
                    texture_path.as_os_str(),
                    stages,
                    eye,
                ],
                "costura",
            ),
        ];
        for (args, needle) in failing {
            let mut all = vec![os("--output"), output.as_os_str()];
            all.extend(args.iter().copied());
            let result = run_game(&all);
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert_eq!(result.status.code(), Some(1), "{args:?}: {stderr}");
            assert!(stderr.contains(needle), "{needle:?} no esta en {stderr}");
            assert!(!output.exists(), "{args:?} genero un PNG");
        }
        // La captura no puede sobrescribir una entrada.
        let mut args = vec![os("--output"), texture_path.as_os_str(), stages, eye];
        args.extend(creature);
        let result = run_game(&args);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("coincide con la entrada"));
        assert_eq!(std::fs::read(&texture_path).unwrap(), texture_bytes);

        let capture = |name: &str, extra: &[&OsStr]| {
            let path = dir.join(name);
            let mut args = vec![os("--output"), path.as_os_str()];
            args.extend(extra.iter().copied());
            let result = run_game(&args);
            assert!(
                result.status.success(),
                "{extra:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            (
                std::fs::read(&path).unwrap(),
                String::from_utf8_lossy(&result.stdout).into_owned(),
            )
        };
        let near = [&creature[..], &[os("--near-creature"), stages]].concat();
        let with_eye = [&near[..], &[eye]].concat();
        let o5 = [os("--debug-oxygen"), os("0.05")];
        // 5 %: arranca ya mirando a la nave (snap determinista) y el ojo cambia el cuadro.
        let (eye5, stdout) = capture("eye5.png", &[&with_eye[..], &o5].concat());
        let bind = parse_obj(&calibrated_source()).unwrap();
        let skin = EyeSkin::calibrate(&bind, &CREATURE_EYE_REGION).unwrap();
        let placement = CreaturePlacement::demo();
        let at_demo = StaticCreature::new(bind, texture(), placement).unwrap();
        let g = skin.aim(&at_demo, placement.spawn_pose().0);
        let line = format!(
            "Mirada: yaw {:+.3} grados, pitch {:+.3} grados hacia la nave",
            g.yaw().to_degrees(),
            g.pitch().to_degrees()
        );
        assert!(
            !g.is_neutral() && stdout.contains(&line),
            "{line}\n{stdout}"
        );
        assert!(stdout.contains("objetos 6 (descartados"), "{stdout}");
        let (stages5, _) = capture("stages5.png", &[&near[..], &o5].concat());
        assert_ne!(eye5, stages5, "el ojo se movio");
        // 100 % (por defecto): oculta; identica a las etapas sin mirada y con 5 objetos.
        let (eye100, stdout) = capture("eye100.png", &with_eye);
        assert!(stdout.contains("objetos 5 (descartados"), "{stdout}");
        let (stages100, _) = capture("stages100.png", &near);
        assert_eq!(eye100, stages100);
    });
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}
