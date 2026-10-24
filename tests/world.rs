use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::framebuffer::Framebuffer,
    scene::{
        ship::{Ship, ShipInput},
        solar_system::SolarSystem,
        world::{SPACE_COLOR, World},
    },
};

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}

#[test]
fn la_orbita_y_la_rotacion_dependen_del_tiempo_y_no_de_los_cuadros() {
    let planet = &SolarSystem::demo().bodies[1];
    let at = |t: f64| (planet.position(t), planet.rotation_angle(t));
    // Misma t, mismo resultado: no hay estado acumulado por cuadro.
    assert_eq!(at(12.5), at(12.5));
    // Una orbita completa regresa al punto de partida a la misma distancia del Sol.
    let period = std::f64::consts::TAU / planet.orbit.angular_speed;
    assert!((planet.position(0.0) - planet.position(period)).length() < 1e-6);
    assert!((planet.position(3.0).length() - planet.orbit.radius).abs() < 1e-9);
    assert!((at(0.0).0 - at(1.0).0).length() > 1.0);
}

#[test]
fn el_tiempo_del_mundo_acumula_dt_y_no_cuadros() {
    let run = |fps: f64| {
        let mut world = new_world();
        for _ in 0..(2.0 * fps) as usize {
            world.update(&ShipInput::default(), 1.0 / fps);
        }
        world.time
    };
    assert!((run(30.0) - run(144.0)).abs() < 1e-9);
}

#[test]
fn la_distancia_recorrida_es_comparable_a_30_y_144_fps() {
    let travel = |fps: f64| {
        let mut ship = Ship::new(Vec3::ZERO, -Vec3::Z);
        let input = ShipInput {
            thrust: 1.0,
            ..ShipInput::default()
        };
        for _ in 0..(3.0 * fps) as usize {
            ship.update(&input, 1.0 / fps);
        }
        ship.position.length()
    };
    let (slow, fast) = (travel(30.0), travel(144.0));
    assert!(slow > 50.0);
    assert!((slow - fast).abs() / fast < 0.02, "{slow} vs {fast}");
}

#[test]
fn la_orientacion_sigue_siendo_ortonormal_tras_muchos_giros() {
    let mut ship = Ship::new(Vec3::ZERO, Vec3::X);
    let input = ShipInput {
        yaw: 1.0,
        pitch: 0.7,
        roll: -0.4,
        ..ShipInput::default()
    };
    for _ in 0..20_000 {
        ship.update(&input, 1.0 / 60.0);
    }
    let (f, u, r) = (ship.forward(), ship.up(), ship.right());
    assert!((f.length() - 1.0).abs() < 1e-9 && f.dot(u).abs() < 1e-9 && f.dot(r).abs() < 1e-9);
    assert!(r.cross(u).dot(-f) > 0.999);
}

#[test]
fn un_dt_invalido_no_mueve_la_nave() {
    let mut ship = Ship::new(Vec3::ZERO, -Vec3::Z);
    let input = ShipInput {
        thrust: 1.0,
        ..ShipInput::default()
    };
    for dt in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        ship.update(&input, dt);
    }
    assert_eq!((ship.position, ship.speed), (Vec3::ZERO, 0.0));
}

#[test]
fn el_descarte_de_objetos_reduce_el_trabajo_sin_cambiar_la_imagen() {
    let mut world = new_world();
    world.place_near(1, 25.0).unwrap();
    // Girar la nave para que parte del sistema quede fuera de vista.
    let turn = ShipInput {
        yaw: 1.0,
        ..ShipInput::default()
    };
    for _ in 0..90 {
        world.update(&turn, 1.0 / 60.0);
    }
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.toggles.frustum_culling = true;
    let with = world.render(&mut fb);
    let image_with = fb.pixels.clone();
    world.toggles.frustum_culling = false;
    let without = world.render(&mut fb);
    assert!(with.objects_culled > 0 && without.objects_culled == 0);
    assert!(with.vertices_processed < without.vertices_processed);
    assert_eq!(with.triangles_submitted, without.triangles_submitted);
    assert_eq!(image_with, fb.pixels);
}

#[test]
fn acercarse_a_un_planeta_lo_agranda_sin_cambiar_de_escena() {
    let mut world = new_world();
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    let far = world.render(&mut fb).fragments_evaluated;
    world.place_near(1, 25.0).unwrap();
    let near = world.render(&mut fb).fragments_evaluated;
    assert!(near > far);
}

#[test]
fn el_nivel_de_detalle_baja_con_la_distancia_y_reduce_triangulos() {
    let mut world = new_world();
    assert_eq!(world.lod_level(1, 10.0, 100.0), 0);
    assert_eq!(world.lod_level(1, 10.0, 500.0), 1);
    assert_eq!(world.lod_level(1, 10.0, 5000.0), 2);
    let tris: Vec<usize> = world.sphere_lods.iter().map(|m| m.faces.len()).collect();
    assert!(tris[0] > tris[1] && tris[1] > tris[2]);

    // Desde el inicio todos los planetas estan lejos: con LOD se rasterizan menos triangulos.
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.toggles.lod = true;
    let with = world.render(&mut fb);
    world.toggles.lod = false;
    assert_eq!(world.lod_level(1, 10.0, 5000.0), 0);
    let without = world.render(&mut fb);
    assert!(with.triangles_submitted < without.triangles_submitted);
    assert!(with.vertices_processed < without.vertices_processed);
}

#[test]
fn la_histeresis_evita_el_parpadeo_en_el_umbral_del_lod() {
    let world = new_world();
    // Umbral entre nivel 1 y 2: 0.01. Con radio 10, distancia 1000 es justo el limite.
    assert_eq!(world.lod_level(1, 10.0, 800.0), 1);
    // Un poco mas lejos que el limite no cambia de nivel todavia...
    assert_eq!(world.lod_level(1, 10.0, 1050.0), 1);
    // ...pero mas alla del margen si.
    assert_eq!(world.lod_level(1, 10.0, 1250.0), 2);
    // Volver un poco bajo el umbral tampoco regresa al nivel fino hasta pasar el margen.
    assert_eq!(world.lod_level(1, 10.0, 950.0), 2);
    assert_eq!(world.lod_level(1, 10.0, 800.0), 1);
}

#[test]
fn descartar_caras_traseras_reduce_triangulos_sin_cambiar_la_imagen() {
    let mut world = new_world();
    world.place_near(1, 25.0).unwrap();
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    world.toggles.backface_culling = true;
    let with = world.render(&mut fb);
    let image_with = fb.pixels.clone();
    world.toggles.backface_culling = false;
    let without = world.render(&mut fb);
    assert!(with.triangles_backface > 1000 && without.triangles_backface == 0);
    assert!(with.triangles_rasterized < without.triangles_rasterized);
    assert_eq!(image_with, fb.pixels);
}
