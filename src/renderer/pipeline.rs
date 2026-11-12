//! `draw_model`: etapa de vertices (modelo-vista-proyeccion), recorte, viewport y
//! rasterizacion con shader por fragmento (UMB-101/102/104/105). Propietario: Persona 1.
//!
//! **Rasterizacion en paralelo (rendimiento):** `draw_model` prepara todos los triangulos en el
//! orden de siempre y, si el trabajo estimado lo justifica, reparte las **filas** de la pantalla
//! entre hilos de forma intercalada (`y % hilos`). Cada hilo recorre todos los triangulos en el
//! mismo orden y solo escribe sus filas, asi que cada pixel ve exactamente la misma secuencia de
//! z-tests y de llamadas al shader que en la ruta secuencial: el cuadro es identico bit a bit.

use std::sync::OnceLock;

use super::{
    clipping::{NEAR_BIT, VertexOut, clip_near, outcode},
    framebuffer::Framebuffer,
    rasterizer::{PreparedTriangle, RasterVertex},
    shader::{Color, FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment},
    stats::RenderStats,
};
use crate::{
    assets::obj::Model,
    math::{Transform, Vec2, Vec3},
    scene::camera::Camera,
};

/// Pixeles de cajas envolventes a partir de los cuales conviene repartir filas entre hilos
/// (por debajo, crear hilos cuesta mas que lo que ahorra).
pub const PARALLEL_MIN_BOX_PIXELS: usize = 24_000;
/// Tope de hilos de rasterizacion: medido con `examples/perf_bench.rs`, 8 rinde igual o mejor
/// que 16 porque crear hilos en Windows cuesta.
pub const MAX_RASTER_THREADS: usize = 8;

/// Hilos disponibles para rasterizar (`1` desactiva el paralelismo). Se puede fijar con la
/// variable de entorno `UMBRAL_THREADS`; por defecto, los nucleos disponibles hasta el tope.
pub fn raster_threads() -> usize {
    static THREADS: OnceLock<usize> = OnceLock::new();
    *THREADS.get_or_init(|| {
        std::env::var("UMBRAL_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
            .clamp(1, MAX_RASTER_THREADS)
    })
}

/// Dibuja `model` en el framebuffer sin limpiarlo ni autoencuadrarlo. La posicion final
/// sale de `proyeccion * vista * transform`; la profundidad usa el z-buffer del framebuffer.
pub fn draw_model(
    framebuffer: &mut Framebuffer,
    model: &Model,
    transform: &Transform,
    material: &Material,
    shader: ShaderKind,
    context: &ShaderContext,
    camera: &Camera,
) -> RenderStats {
    let (mut stats, triangles) = prepare(framebuffer, model, transform, material, camera);
    let shade =
        |fragment: &FragmentInput, face: &Material| shade_fragment(shader, fragment, context, face);
    let (fragments, written) = rasterize(framebuffer, &triangles, &shade);
    stats.fragments_evaluated += fragments;
    stats.pixels_written += written;
    stats
}

/// Igual que `draw_model`, pero cada fragmento que pasa el z-test se colorea con
/// `shade(fragmento, material_de_la_cara)` (el `Kd` del MTL ya resuelto si el material lo
/// pide). Punto de extension opt-in: la escena del juego sigue usando `draw_model`. Siempre
/// secuencial, porque `shade` puede tener estado.
pub fn draw_model_with(
    framebuffer: &mut Framebuffer,
    model: &Model,
    transform: &Transform,
    material: &Material,
    camera: &Camera,
    mut shade: impl FnMut(&FragmentInput, &Material) -> Color,
) -> RenderStats {
    let (mut stats, triangles) = prepare(framebuffer, model, transform, material, camera);
    for (triangle, face_material) in &triangles {
        let mut shade_face = |fragment: &FragmentInput| shade(fragment, face_material);
        for y in triangle.y_min..=triangle.y_max {
            let (pixels, depth) = framebuffer.row_mut(y as usize);
            let (f, w) = triangle.shade_row(y, pixels, depth, &mut shade_face);
            stats.fragments_evaluated += f;
            stats.pixels_written += w;
        }
    }
    stats
}

/// Etapa de vertices, recorte, viewport y descarte de caras: los triangulos de pantalla en el
/// orden de dibujo (caras en orden y cada abanico en orden) con el material de su cara.
fn prepare(
    framebuffer: &Framebuffer,
    model: &Model,
    transform: &Transform,
    material: &Material,
    camera: &Camera,
) -> (RenderStats, Vec<(PreparedTriangle, Material)>) {
    let mut stats = RenderStats {
        objects_submitted: 1,
        vertices_processed: model.vertices.len(),
        ..RenderStats::default()
    };
    let model_matrix = transform.matrix();
    let mvp = camera.view_projection() * model_matrix;
    // Etapa de vertices una sola vez por vertice, no por esquina de cara.
    let clip: Vec<_> = model
        .vertices
        .iter()
        .map(|&v| mvp.transform(Vec3::from_array(v).extend(1.0)))
        .collect();
    let world: Vec<_> = model
        .vertices
        .iter()
        .map(|&v| model_matrix.transform_point(Vec3::from_array(v)))
        .collect();
    let normals: Vec<_> = model
        .normals
        .iter()
        .map(|&n| transform.transform_normal(Vec3::from_array(n)))
        .collect();
    let (fb_width, fb_height) = (framebuffer.width, framebuffer.height);
    let (width, height) = (fb_width as f64, fb_height as f64);
    let mut triangles = Vec::new();

    for face in &model.faces {
        stats.triangles_submitted += 1;
        let [i0, i1, i2] = face.vertex_indices;
        let codes = [outcode(clip[i0]), outcode(clip[i1]), outcode(clip[i2])];
        if codes[0] & codes[1] & codes[2] != 0 {
            stats.triangles_culled += 1;
            continue;
        }
        let flat = (world[i1] - world[i0])
            .cross(world[i2] - world[i0])
            .normalized();
        let corner = |slot: usize, vertex: usize| VertexOut {
            clip_position: clip[vertex],
            world_position: world[vertex],
            world_normal: face.normal_indices.map_or(flat, |n| normals[n[slot]]),
            uv: face.uv_indices.map_or(Vec2::default(), |t| {
                let uv = model.uvs[t[slot]];
                Vec2::new(uv[0], uv[1])
            }),
        };
        let tri = [corner(0, i0), corner(1, i1), corner(2, i2)];
        let clipped;
        let polygon: &[VertexOut] = if (codes[0] | codes[1] | codes[2]) & NEAR_BIT != 0 {
            stats.triangles_clipped += 1;
            clipped = clip_near(tri);
            &clipped
        } else {
            &tri
        };
        if polygon.len() < 3 {
            stats.triangles_culled += 1;
            continue;
        }

        let face_material = Material {
            base_color: match (&face.material, material.use_model_colors) {
                (Some(name), true) => model
                    .materials
                    .get(name)
                    .map_or(material.base_color, |&kd| Vec3::from_array(kd)),
                _ => material.base_color,
            },
            ..*material
        };
        let to_screen = |v: &VertexOut| {
            let c = v.clip_position;
            let inverse_w = 1.0 / c.w;
            RasterVertex {
                x: (c.x * inverse_w * 0.5 + 0.5) * width,
                y: (0.5 - c.y * inverse_w * 0.5) * height,
                z: c.z * inverse_w,
                inverse_w,
                world_position: v.world_position,
                normal: v.world_normal,
                uv: v.uv,
            }
        };
        let first = to_screen(&polygon[0]);
        if material.cull_backfaces {
            // En pantalla (Y hacia abajo) un triangulo antihorario de frente tiene area negativa.
            let (a, b, c) = (first, to_screen(&polygon[1]), to_screen(&polygon[2]));
            let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if area >= 0.0 {
                stats.triangles_backface += 1;
                continue;
            }
        }
        // Abanico: un poligono de n vertices produce n - 2 triangulos.
        for k in 1..polygon.len() - 1 {
            stats.triangles_rasterized += 1;
            let screen = [first, to_screen(&polygon[k]), to_screen(&polygon[k + 1])];
            if let Some(triangle) = PreparedTriangle::new(screen, fb_width, fb_height) {
                triangles.push((triangle, face_material));
            }
        }
    }
    (stats, triangles)
}

/// Rasteriza en orden; con trabajo suficiente reparte filas intercaladas entre hilos.
/// Devuelve `(fragmentos evaluados, pixeles escritos)`, iguales a los de la ruta secuencial.
fn rasterize(
    framebuffer: &mut Framebuffer,
    triangles: &[(PreparedTriangle, Material)],
    shade: &(impl Fn(&FragmentInput, &Material) -> Color + Sync),
) -> (usize, usize) {
    let work: usize = triangles.iter().map(|(t, _)| t.box_area()).sum();
    let threads = raster_threads().min(framebuffer.height);
    if threads <= 1 || work < PARALLEL_MIN_BOX_PIXELS {
        let (mut fragments, mut written) = (0, 0);
        for (triangle, face_material) in triangles {
            let mut shade_face = |fragment: &FragmentInput| shade(fragment, face_material);
            for y in triangle.y_min..=triangle.y_max {
                let (pixels, depth) = framebuffer.row_mut(y as usize);
                let (f, w) = triangle.shade_row(y, pixels, depth, &mut shade_face);
                fragments += f;
                written += w;
            }
        }
        return (fragments, written);
    }
    // Fila `y` va al hilo `y % threads`, en la posicion `y / threads` de su lista.
    let mut buckets: Vec<Vec<(&mut [u32], &mut [f64])>> =
        (0..threads).map(|_| Vec::new()).collect();
    for (y, row) in framebuffer.rows_mut().enumerate() {
        buckets[y % threads].push(row);
    }
    std::thread::scope(|scope| {
        let handles: Vec<_> = buckets
            .into_iter()
            .enumerate()
            .map(|(t, mut rows)| {
                scope.spawn(move || {
                    let (mut fragments, mut written) = (0, 0);
                    for (triangle, face_material) in triangles {
                        let (y_min, y_max) = (triangle.y_min, triangle.y_max);
                        if y_max < y_min {
                            continue;
                        }
                        let mut shade_face =
                            |fragment: &FragmentInput| shade(fragment, face_material);
                        // Primera fila de este hilo dentro del rango del triangulo.
                        let mut y = y_min + (t as isize - y_min).rem_euclid(threads as isize);
                        while y <= y_max {
                            let (pixels, depth) = &mut rows[y as usize / threads];
                            let (f, w) = triangle.shade_row(y, pixels, depth, &mut shade_face);
                            fragments += f;
                            written += w;
                            y += threads as isize;
                        }
                    }
                    (fragments, written)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("hilo de rasterizacion"))
            .fold((0, 0), |(f, w), (a, b)| (f + a, w + b))
    })
}
