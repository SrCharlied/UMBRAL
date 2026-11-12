//! Criatura estatica opcional en el juego: colocacion, carga, linea de comandos e
//! integracion con `World`. Fixtures sinteticos y pequenos creados aqui; no necesita el
//! asset real ni abre ventanas.

use std::{
    f64::consts::TAU,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::{
    assets::{
        obj::{Model, load_model, parse_obj},
        ship_model_path,
    },
    math::{Vec2, Vec3},
    renderer::{
        framebuffer::Framebuffer, pipeline::draw_model_with, shader::Material, stats::RenderStats,
        texture::Texture,
    },
    scene::{
        ship::ShipInput,
        solar_system::SolarSystem,
        world::{SPACE_COLOR, World},
    },
    visual::{
        static_creature::*,
        textured::{TexturedMode, TexturedShader},
    },
};

fn close_vec(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    (a - b).length() <= tolerance
}

/// Cubo de lado `2 * half` centrado en `center`, con UV completos por cara y normales.
fn cube_source(center: Vec3, half: f64) -> String {
    let mut source = String::new();
    for (x, y, z) in [
        (-1.0, -1.0, -1.0),
        (1.0, -1.0, -1.0),
        (1.0, 1.0, -1.0),
        (-1.0, 1.0, -1.0),
        (-1.0, -1.0, 1.0),
        (1.0, -1.0, 1.0),
        (1.0, 1.0, 1.0),
        (-1.0, 1.0, 1.0),
    ] {
        source += &format!(
            "v {} {} {}\n",
            center.x + x * half,
            center.y + y * half,
            center.z + z * half
        );
    }
    source += "vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n";
    source += "vn 0 0 1\nvn 0 0 -1\nvn 1 0 0\nvn -1 0 0\nvn 0 1 0\nvn 0 -1 0\n";
    for (n, [a, b, c, d]) in [
        [5, 6, 7, 8],
        [2, 1, 4, 3],
        [6, 2, 3, 7],
        [1, 5, 8, 4],
        [8, 7, 3, 4],
        [1, 2, 6, 5],
    ]
    .into_iter()
    .enumerate()
    {
        let n = n + 1;
        source += &format!("f {a}/1/{n} {b}/2/{n} {c}/3/{n}\nf {a}/1/{n} {c}/3/{n} {d}/4/{n}\n");
    }
    source
}

/// Modelo con el centro envolvente lejos del origen, como el OBJ de la criatura.
fn uncentered_cube() -> Model {
    parse_obj(&cube_source(Vec3::new(5.0, -3.0, 2.0), 1.5)).unwrap()
}

fn rgb_texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![230, 40, 40, 40, 220, 60, 50, 80, 240, 240, 220, 40],
    )
    .unwrap()
}

#[test]
fn placement_maps_uncentered_model_bounds_onto_the_chosen_world_sphere() {
    let model = uncentered_cube();
    let (c, r) = (Vec3::from_array(model.bounds_center), model.bounds_radius);
    assert!(c.length() > 5.0, "el fixture debe estar descentrado");
    for yaw in [0.0, 1.0, -2.5] {
        let placement = CreaturePlacement::new(Vec3::new(10.0, -20.0, -300.0), 40.0, yaw).unwrap();
        let transform = placement.transform_for(c, r).unwrap();
        let m = transform.matrix();
        assert!(close_vec(m.transform_point(c), placement.center(), 1e-9));
        assert_eq!(transform.scale, Vec3::splat(40.0 / r));
        assert_eq!(transform.rotation, Vec3::new(0.0, yaw, 0.0));
        let farthest = model
            .vertices
            .iter()
            .map(|&v| (m.transform_point(Vec3::from_array(v)) - placement.center()).length())
            .fold(0.0, f64::max);
        assert!((farthest - 40.0).abs() < 1e-9, "radio mundial {farthest}");
        // El frente del OBJ (+Z) queda hacia `front()`.
        assert!(close_vec(
            m.transform_dir(Vec3::Z).normalized(),
            placement.front(),
            1e-12
        ));
    }
    let front = CreaturePlacement::new(Vec3::ZERO, 1.0, 0.0)
        .unwrap()
        .front();
    assert_eq!(front, Vec3::Z);
}

#[test]
fn placement_rejects_invalid_values_and_extremes_stay_finite() {
    let invalid = [
        (Vec3::new(f64::NAN, 0.0, 0.0), 10.0, 0.0),
        (Vec3::new(0.0, f64::INFINITY, 0.0), 10.0, 0.0),
        (Vec3::new(0.0, 0.0, -MAX_WORLD_COORD * 1.01), 10.0, 0.0),
        (Vec3::ZERO, f64::NAN, 0.0),
        (Vec3::ZERO, f64::INFINITY, 0.0),
        (Vec3::ZERO, 0.0, 0.0),
        (Vec3::ZERO, -5.0, 0.0),
        (Vec3::ZERO, MIN_WORLD_RADIUS * 0.5, 0.0),
        (Vec3::ZERO, MAX_WORLD_RADIUS * 2.0, 0.0),
        (Vec3::ZERO, 10.0, f64::NAN),
        (Vec3::ZERO, 10.0, f64::NEG_INFINITY),
        (Vec3::ZERO, 10.0, TAU + 1e-9),
    ];
    for (center, radius, yaw) in invalid {
        assert!(
            CreaturePlacement::new(center, radius, yaw).is_err(),
            "aceptado: {center:?} {radius} {yaw}"
        );
    }
    let placement = CreaturePlacement::new(Vec3::splat(1.0), 10.0, 0.0).unwrap();
    for (c, r) in [
        (Vec3::new(f64::NAN, 0.0, 0.0), 1.0),
        (Vec3::ZERO, 0.0),
        (Vec3::ZERO, f64::INFINITY),
        (Vec3::splat(1e9), 0.5),
    ] {
        assert!(placement.transform_for(c, r).is_err(), "modelo {c:?} r {r}");
    }
    for (center, radius, yaw) in [
        (Vec3::splat(MAX_WORLD_COORD), MAX_WORLD_RADIUS, TAU),
        (Vec3::splat(-MAX_WORLD_COORD), MIN_WORLD_RADIUS, -TAU),
    ] {
        let placement = CreaturePlacement::new(center, radius, yaw).unwrap();
        for (c, r) in [
            (Vec3::splat(1e9), 1e9),
            (Vec3::ZERO, 1e-6),
            (Vec3::splat(-1e3), 1.0),
        ] {
            let m = placement.transform_for(c, r).unwrap().matrix();
            assert!(
                m.m.iter().flatten().all(|v| v.is_finite()),
                "{center:?} {c:?} {r}"
            );
            assert!(m.transform_point(c).is_finite());
        }
    }
}

#[test]
fn demo_placement_is_outside_every_orbit_and_inside_the_far_plane() {
    let demo = CreaturePlacement::demo();
    assert_eq!(demo.center(), Vec3::new(0.0, 0.0, -900.0));
    assert_eq!((demo.world_radius(), demo.yaw()), (80.0, 0.0));
    assert_eq!(
        demo,
        CreaturePlacement::new(DEMO_CENTER, DEMO_WORLD_RADIUS, 0.0).unwrap()
    );
    // Ningun cuerpo, en ningun instante, puede tocar la esfera de la criatura.
    let system = SolarSystem::demo();
    let orbital_extent = system
        .bodies
        .iter()
        .map(|b| b.orbit.radius + b.body_radius)
        .fold(0.0, f64::max);
    let gap = demo.center().length() - demo.world_radius() - orbital_extent;
    assert!(gap > 100.0, "separacion {gap}");
    let world = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    // Visible a distancia desde la nave: toda la esfera dentro del plano lejano.
    assert!(demo.center().length() + demo.world_radius() < world.camera.far * 0.5);
}

#[test]
fn creature_owns_model_and_texture_and_derives_a_static_transform_from_bounds() {
    let model = uncentered_cube();
    let (c, r) = (Vec3::from_array(model.bounds_center), model.bounds_radius);
    let vertices = model.vertices.clone();
    let placement = CreaturePlacement::new(Vec3::new(0.0, 5.0, -400.0), 30.0, 0.4).unwrap();
    let creature = StaticCreature::new(model, rgb_texture(), placement).unwrap();
    assert_eq!(creature.transform(), placement.transform_for(c, r).unwrap());
    assert_eq!(creature.placement(), placement);
    assert_eq!(creature.model().vertices, vertices);
    assert_eq!(
        (creature.texture().width(), creature.texture().height()),
        (2, 2)
    );

    // Bounds invalidos del modelo: todos los vertices en el mismo punto (radio 0).
    let point = parse_obj("v 1 1 1\nv 1 1 1\nv 1 1 1\nf 1 2 3\n").unwrap();
    assert!(StaticCreature::new(point, rgb_texture(), placement).is_err());
}

// --- Carga y configuracion ---

/// Carpeta temporal propia de cada prueba; se borra al final.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-static-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_png(path: &Path, width: u32, height: u32, color: png::ColorType, bytes: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(bytes).unwrap();
    writer.finish().unwrap();
}

/// OBJ (cubo descentrado) y PNG RGB8 2x2 sinteticos en `dir`.
fn write_assets(dir: &Path) -> (PathBuf, PathBuf) {
    let (model, texture) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, cube_source(Vec3::new(5.0, -3.0, 2.0), 1.5)).unwrap();
    let rgb = [230, 40, 40, 40, 220, 60, 50, 80, 240, 240, 220, 40];
    write_png(&texture, 2, 2, png::ColorType::Rgb, &rgb);
    (model, texture)
}

#[test]
fn load_reads_model_and_texture_and_reports_the_failing_path() {
    let dir = temp_dir("load");
    let (model, texture) = write_assets(&dir);
    let rgba = dir.join("rgba.png");
    write_png(&rgba, 1, 1, png::ColorType::Rgba, &[1, 2, 3, 4]);
    let placement = CreaturePlacement::demo();
    let outcome = std::panic::catch_unwind(|| {
        let creature = StaticCreature::load(&model, &texture, placement).unwrap();
        let parsed = parse_obj(&cube_source(Vec3::new(5.0, -3.0, 2.0), 1.5)).unwrap();
        let expected = StaticCreature::new(parsed, rgb_texture(), placement).unwrap();
        assert_eq!(creature.transform(), expected.transform());
        assert_eq!(creature.model().faces.len(), 12);
        let uv = Vec2::new(0.25, 0.75);
        assert_eq!(creature.texture().sample(uv), rgb_texture().sample(uv));

        let error = StaticCreature::load(&model, &dir.join("no-existe.png"), placement)
            .err()
            .unwrap();
        assert!(error.contains("no-existe.png"), "{error}");
        let error = StaticCreature::load(&dir.join("no-existe.obj"), &texture, placement)
            .err()
            .unwrap();
        assert!(error.contains("no-existe.obj"), "{error}");
        let error = StaticCreature::load(&model, &rgba, placement)
            .err()
            .unwrap();
        assert!(error.contains("no admitido"), "{error}");
    });
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}

#[test]
fn creature_flags_are_validated_before_any_io() {
    let p = |s: &str| Some(PathBuf::from(s));
    // Sin flags: juego normal sin criatura.
    assert_eq!(
        CreatureOptions::from_flags(None, None, false, None, false),
        Ok(None)
    );
    assert_eq!(
        CreatureOptions::from_flags(None, None, false, Some(1), true),
        Ok(None)
    );
    assert_eq!(
        CreatureOptions::from_flags(p("a.obj"), p("b.png"), true, None, false),
        Ok(Some(CreatureOptions {
            model: PathBuf::from("a.obj"),
            texture: PathBuf::from("b.png"),
            near: true,
        }))
    );
    let invalid = [
        (p("a.obj"), None, false, None, false, "juntos"),
        (None, p("b.png"), false, None, false, "juntos"),
        (None, None, true, None, false, "--near-creature"),
        (p("a.obj"), p("b.png"), true, Some(1), false, "--near"),
        (p("a.obj"), p("b.png"), false, None, true, "--legacy"),
        (None, None, true, None, true, "--legacy"),
    ];
    for (model, texture, near_creature, near, legacy, needle) in invalid {
        let error = CreatureOptions::from_flags(
            model.clone(),
            texture.clone(),
            near_creature,
            near,
            legacy,
        )
        .expect_err(&format!(
            "aceptado: {model:?} {texture:?} {near_creature} {near:?} {legacy}"
        ));
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
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
fn the_game_command_line_wires_the_creature_and_rejects_bad_combinations() {
    let dir = temp_dir("cli");
    let (model, texture) = write_assets(&dir);
    let texture_bytes = std::fs::read(&texture).unwrap();
    let output = dir.join("salida.png");
    let missing_texture = dir.join("no-existe.png");
    let os = OsStr::new;
    let outcome = std::panic::catch_unwind(|| {
        let failing: [(Vec<&OsStr>, &str); 6] = [
            (vec![os("--creature-model"), model.as_os_str()], "juntos"),
            (vec![os("--near-creature")], "--near-creature"),
            // Combinacion invalida con rutas inexistentes: el error es de validacion, no de IO.
            (
                vec![
                    os("--creature-model"),
                    os("no-existe.obj"),
                    os("--creature-texture"),
                    os("no-existe.png"),
                    os("--near"),
                    os("1"),
                    os("--near-creature"),
                ],
                "--near",
            ),
            (
                vec![
                    os("--legacy"),
                    os("--creature-model"),
                    os("no-existe.obj"),
                    os("--creature-texture"),
                    os("no-existe.png"),
                ],
                "--legacy",
            ),
            (
                vec![os("--creature-model")],
                "Falta la ruta despues de --creature-model",
            ),
            (
                vec![
                    os("--creature-model"),
                    model.as_os_str(),
                    os("--creature-texture"),
                    missing_texture.as_os_str(),
                ],
                "no-existe.png",
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
        let result = run_game(&[
            os("--output"),
            texture.as_os_str(),
            os("--creature-model"),
            model.as_os_str(),
            os("--creature-texture"),
            texture.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("coincide con la entrada"), "{stderr}");
        assert_eq!(std::fs::read(&texture).unwrap(), texture_bytes);

        // Cableado completo: carga, coloca la nave frente a la criatura y la dibuja.
        let result = run_game(&[
            os("--output"),
            output.as_os_str(),
            os("--creature-model"),
            model.as_os_str(),
            os("--creature-texture"),
            texture.as_os_str(),
            os("--near-creature"),
        ]);
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{stderr}");
        assert!(
            stdout.contains("Criatura: 8 vertices, 12 triangulos, textura 2x2"),
            "{stdout}"
        );
        // CORRECTION03: +6 balizas integradas en el cuadro del CLI.
        assert!(stdout.contains("objetos 12 (descartados"), "{stdout}");
        assert!(output.exists());
    });
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}

// --- Integracion con World ---

const W: usize = 320;
const H: usize = 180;

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

fn creature_at(center: Vec3, radius: f64, yaw: f64) -> StaticCreature {
    let placement = CreaturePlacement::new(center, radius, yaw).unwrap();
    StaticCreature::new(uncentered_cube(), rgb_texture(), placement).unwrap()
}

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Contadores que no dependen del orden de dibujo ni del z-test.
fn additive(s: &RenderStats) -> [usize; 8] {
    [
        s.objects_submitted,
        s.objects_culled,
        s.vertices_processed,
        s.triangles_submitted,
        s.triangles_culled,
        s.triangles_clipped,
        s.triangles_backface,
        s.triangles_rasterized,
    ]
}

fn sub(a: [usize; 8], b: [usize; 8]) -> [usize; 8] {
    std::array::from_fn(|i| a[i] - b[i])
}

/// Un triangulo degenerado (area cero, no cubre pixeles): oculta la nave sin tocar su camara.
fn no_ship() -> Model {
    parse_obj("v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3\n").unwrap()
}

#[test]
fn without_a_creature_the_world_renders_exactly_as_before() {
    let mut world = without_catalogs(new_world());
    assert!(world.creature().is_none());
    let (pixels, stats) = render(&world);
    assert_eq!(stats.objects_submitted, 5, "Sol, 3 planetas y nave");
    world
        .set_creature(
            StaticCreature::new(uncentered_cube(), rgb_texture(), CreaturePlacement::demo())
                .unwrap(),
        )
        .unwrap();
    let (_, with) = render(&world);
    assert_eq!(with.objects_submitted, 6);
    assert!(world.clear_creature().is_some() && world.creature().is_none());
    assert_eq!(render(&world), (pixels, stats));
}

#[test]
fn a_visible_creature_uses_the_ship_camera_sun_light_and_shared_depth() {
    let mut world = new_world();
    world
        .set_creature(creature_at(DEMO_CENTER, DEMO_WORLD_RADIUS, 0.0))
        .unwrap();
    world.place_near_creature().unwrap();
    assert!(
        world.toggles.backface_culling,
        "culling de caras traseras global activo"
    );
    let (with_pixels, with) = render(&world);
    let creature = world.clear_creature().unwrap();
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let without = world.render(&mut fb);
    assert_ne!(with_pixels, fb.pixels, "la criatura no se ve");

    // Mismo resultado que dibujar la criatura sobre el cuadro sin ella, con el z-buffer
    // intacto, la camara de la nave, la luz del Sol, ambiente 0.25 y doble cara.
    let shader = TexturedShader::new(
        creature.texture(),
        TexturedMode::Lit,
        world.system.light_position(),
        AMBIENT,
    )
    .unwrap();
    let alone = draw_model_with(
        &mut fb,
        creature.model(),
        &creature.transform(),
        &Material::from_model(),
        &world.camera,
        |f, _| shader.shade(f),
    );
    assert_eq!(with_pixels, fb.pixels);
    assert_eq!(sub(additive(&with), additive(&without)), additive(&alone));
    assert_eq!((alone.objects_submitted, alone.vertices_processed), (1, 8));
    assert_eq!(alone.triangles_backface, 0, "malla abierta: doble cara");
    assert!(alone.triangles_rasterized > 0);
}

#[test]
fn frustum_culling_skips_an_off_screen_creature_and_no_culling_pays_its_vertices() {
    // La nave arranca en (0, 40, 700) mirando a -Z: esta criatura queda detras de la camara.
    let behind = || creature_at(Vec3::new(0.0, 40.0, 1200.0), 50.0, 0.0);
    for culling in [true, false] {
        let mut world = new_world();
        world.toggles.frustum_culling = culling;
        let (pixels, without) = render(&world);
        world.set_creature(behind()).unwrap();
        let (with_pixels, with) = render(&world);
        assert_eq!(with_pixels, pixels, "culling {culling}");
        let delta = sub(additive(&with), additive(&without));
        let expected = if culling {
            // Descartada entera antes de transformar: 0 vertices, 0 rasterizados.
            [1, 1, 0, 12, 12, 0, 0, 0]
        } else {
            // Sin descarte paga la etapa de vertices y la descartan los planos del recorte.
            [1, 0, 8, 12, 12, 0, 0, 0]
        };
        assert_eq!(delta, expected, "culling {culling}");
        assert_eq!(with.fragments_evaluated, without.fragments_evaluated);
    }
}

#[test]
fn depth_is_shared_with_planets_and_ship_and_the_frame_is_not_cleared() {
    let mut world = new_world();
    world.place_near(1, 25.0).unwrap();
    let planet = world.system.bodies[1].position(world.time);
    let eye = world.camera.position;
    let to_planet = (planet - eye).normalized();
    let surface = (planet - eye).length() - world.system.bodies[1].body_radius;
    let (base_pixels, base) = render(&world);

    // Detras del planeta, en la misma linea de vista: se rasteriza pero el z-test la oculta.
    world
        .set_creature(creature_at(planet + to_planet * 30.0, 3.0, 0.0))
        .unwrap();
    let (hidden_pixels, hidden) = render(&world);
    assert!(hidden.triangles_rasterized > base.triangles_rasterized);
    assert_eq!(hidden_pixels, base_pixels);

    // Delante del planeta: lo tapa, y fuera de su circulo proyectado nada cambia.
    let center = eye + to_planet * (surface * 0.8);
    let radius = 2.0;
    world
        .set_creature(creature_at(center, radius, 0.0))
        .unwrap();
    let (front_pixels, _) = render(&world);
    let clip = world.camera.view_projection().transform(center.extend(1.0));
    let (cx, cy) = (
        (clip.x / clip.w * 0.5 + 0.5) * W as f64,
        (0.5 - clip.y / clip.w * 0.5) * H as f64,
    );
    let distance = (center - eye).length();
    let pixel_radius = radius / (distance * (world.camera.fov_y / 2.0).tan()) * H as f64 / 2.0;
    let mut changed_over_planet = 0;
    for (i, (&a, &b)) in front_pixels.iter().zip(&base_pixels).enumerate() {
        if a == b {
            continue;
        }
        let (x, y) = ((i % W) as f64 + 0.5, (i / W) as f64 + 0.5);
        assert!(
            (x - cx).hypot(y - cy) <= pixel_radius * 1.25 + 1.0,
            "cambio fuera de la criatura en ({x}, {y})"
        );
        changed_over_planet += usize::from(b != SPACE_COLOR);
    }
    assert!(
        changed_over_planet > 20,
        "no tapa al planeta: {changed_over_planet}"
    );

    // La nave (mas cerca) tapa a la criatura; sin nave, la criatura ocupa esos pixeles.
    let mut world = new_world();
    world
        .set_creature(creature_at(DEMO_CENTER, DEMO_WORLD_RADIUS, 0.0))
        .unwrap();
    world.place_near_creature().unwrap();
    let (with, _) = render(&world);
    let ship_model = std::mem::replace(&mut world.ship_model, no_ship());
    let (with_no_ship, _) = render(&world);
    let creature = world.clear_creature().unwrap();
    let (without_no_ship, _) = render(&world);
    world.ship_model = ship_model;
    let (without, _) = render(&world);
    world.set_creature(creature).unwrap();
    let ship_mask: Vec<usize> = (0..W * H)
        .filter(|&i| without[i] != without_no_ship[i])
        .collect();
    assert!(ship_mask.len() > 100, "la nave no se ve");
    assert!(
        ship_mask.iter().all(|&i| with[i] == without[i]),
        "la criatura tapa la nave"
    );
    let behind_ship = ship_mask
        .iter()
        .filter(|&&i| with_no_ship[i] != without_no_ship[i])
        .count();
    assert!(
        behind_ship > 50,
        "la criatura no queda detras de la nave: {behind_ship}"
    );
}

#[test]
fn the_creature_stays_static_while_time_and_the_ship_camera_move() {
    let mut world = new_world();
    world
        .set_creature(creature_at(DEMO_CENTER, DEMO_WORLD_RADIUS, 0.3))
        .unwrap();
    world.place_near_creature().unwrap();
    let transform = world.creature().unwrap().transform();
    let vertices = world.creature().unwrap().model().vertices.clone();
    let camera = world.camera.position;
    let input = ShipInput {
        thrust: 1.0,
        yaw: 0.5,
        pitch: -0.2,
        ..ShipInput::default()
    };
    for _ in 0..120 {
        world.update(&input, 1.0 / 60.0);
    }
    assert!(world.time > 1.9);
    assert!(
        (world.camera.position - camera).length() > 1.0,
        "la camara no se movio"
    );
    let creature = world.creature().unwrap();
    assert_eq!(creature.transform(), transform);
    assert_eq!(creature.model().vertices, vertices);
}

#[test]
fn near_creature_spawn_faces_its_front_and_can_be_repeated_exactly() {
    let mut world = new_world();
    assert!(
        world.place_near_creature().is_err(),
        "sin criatura no hay arranque"
    );
    let bodies_before: Vec<_> = world
        .system
        .bodies
        .iter()
        .map(|b| (b.body_radius, b.orbit.radius, b.position(0.0)))
        .collect();
    let placement = CreaturePlacement::new(Vec3::new(100.0, -50.0, -700.0), 60.0, 0.7).unwrap();
    world
        .set_creature(StaticCreature::new(uncentered_cube(), rgb_texture(), placement).unwrap())
        .unwrap();
    world.time = 3.0;
    world.place_near_creature().unwrap();
    assert_eq!(world.time, 3.0, "el arranque no cambia el tiempo");
    let (ship, camera) = (world.ship, world.camera);
    let center = placement.center();
    let offset = ship.position - center;
    assert!(
        offset.dot(placement.front()) > 0.0,
        "la nave queda del lado del frente"
    );
    assert!((offset.length() - SPAWN_DISTANCE_RADII * 60.0).abs() < 1e-9);
    assert!((ship.forward().dot((center - ship.position).normalized()) - 1.0).abs() < 1e-9);
    // La esfera entera cabe en el campo de vision vertical de la camara de la nave.
    let to_center = center - camera.position;
    let off_axis = camera
        .forward()
        .dot(to_center.normalized())
        .clamp(-1.0, 1.0)
        .acos();
    let angular_radius = (60.0 / to_center.length()).asin();
    assert!(
        off_axis + angular_radius < camera.fov_y / 2.0,
        "no cabe: {off_axis} {angular_radius}"
    );

    for _ in 0..90 {
        world.update(
            &ShipInput {
                thrust: 1.0,
                yaw: 1.0,
                ..ShipInput::default()
            },
            1.0 / 60.0,
        );
    }
    world.place_near_creature().unwrap();
    assert_eq!(world.ship.position, ship.position);
    assert_eq!(world.ship.orientation, ship.orientation);
    assert_eq!(world.camera.position, camera.position);
    assert_eq!(world.camera.target, camera.target);
    let bodies_after: Vec<_> = world
        .system
        .bodies
        .iter()
        .map(|b| (b.body_radius, b.orbit.radius, b.position(0.0)))
        .collect();
    assert_eq!(bodies_after, bodies_before);
}

/// CORRECTION03: componente visual puro con los catalogos de gameplay EXPLICITAMENTE vacios (sin las seis
/// balizas integradas en `World::new`); la escena integrada con balizas se prueba aparte.
fn without_catalogs(mut world: World) -> World {
    world.gameplay.resources.clear();
    world.gameplay.logs.clear();
    world
}

#[test]
fn correction03_the_integrated_world_adds_exactly_its_six_beacons() {
    let world = new_world();
    let beacons = world.gameplay.resources.len() + world.gameplay.logs.len();
    assert_eq!(beacons, 6);
    let (_, plain) = render(&without_catalogs(new_world()));
    let (_, integrated) = render(&world);
    assert_eq!(plain.objects_submitted, 5, "Sol, 3 planetas y nave");
    assert_eq!(
        integrated.objects_submitted,
        plain.objects_submitted + beacons
    );
}
