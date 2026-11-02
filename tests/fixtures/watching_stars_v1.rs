//! UMB-310 preview AISLADO: estrellas observadoras («Pupilas de alfiler»). Las MISMAS 700
//! estrellas de `draw_stars` del World abren, segun baja el O2, un ojo diminuto cuya pupila mira a
//! la nave proyectada. Funcion pura sobre el cuadro ya dibujado: no toca la fuente ni su
//! profundidad. Ver `docs/watching-stars-preview.md`.
use umbral::{
    math::{Mat4, Vec3},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        shader::to_rgb8,
    },
    scene::camera::Camera,
};

/// Estrellas del cielo publicado (`draw_stars`, `COUNT`).
pub const STAR_COUNT: usize = 700;

/// Con este O2 o mas no hay ninguna observadora: el cuadro sale EXACTO. Es una banda tecnica de
/// continuidad por encima del 60 % pedido: entre 65 y 60 % las primeras abren poco a poco, para que
/// al 60 % ya se vean sin aparecer de golpe.
pub const OBSERVER_ONSET_OXYGEN: f64 = 0.65;
/// Fundido de cada estrella: desde su umbral, tarda este O2 en abrir del todo (suave, sin pulsos).
/// Calibrado a 3 % (la candidata empezo en 5 %) junto con el primer nodo: ver la curva.
pub const OBSERVER_FADE_OXYGEN: f64 = 0.03;
/// Curva CANDIDATA (no fijada por Charlie): umbral de aparicion segun la fraccion del ranking
/// `(k + 0.5) / 700`, interpolada con Hermite monotona (PCHIP). Lectura: una estrella de rank `k`
/// empieza a abrir cuando el O2 baja de su umbral y esta del todo abierta 3 % despues.
/// - rank < 28 (4 %, ~18 legibles = 2,6 %) empieza por encima del 60 %: 1 a 3 por encuadre. La
///   candidata inicial (16 y fundido 5 %) dejaba 0 en el arranque normal: su unica observadora en
///   cuadro (rank 2) queda detras de la nave. Ajuste minimo medido, documentado en el informe;
/// - al 50 % abre la que tiene umbral > 50 % (24 %), legibles ~20 %: claramente mas, no casi todo;
/// - al 25 % abre el 96 % (legibles ~94 %): casi todas; en el 10 % la ultima tiene su umbral.
pub const THRESHOLD_KNOTS: [(f64, f64); 5] = [
    (0.0, OBSERVER_ONSET_OXYGEN),
    (28.0 / STAR_COUNT as f64, 0.6),
    (0.24, 0.5),
    (0.96, 0.25),
    (1.0, 0.1),
];

/// Una estrella del cielo publicado (`draw_stars` del World).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatalogStar {
    pub id: usize,
    pub direction: Vec3,
    pub color: u32,
}

/// Hash del cielo publicado (`draw_stars`), copiado sin cambios.
fn sky_hash(n: u32) -> f64 {
    let mut x = n.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x as f64 / u32::MAX as f64
}

/// Las 700 estrellas de `draw_stars` (mismos IDs, direcciones y colores, en su orden de dibujo).
pub fn star_catalog() -> Vec<CatalogStar> {
    (0..STAR_COUNT as u32)
        .map(|i| {
            let z = sky_hash(i * 4) * 2.0 - 1.0;
            let a = sky_hash(i * 4 + 1) * std::f64::consts::TAU;
            let r = (1.0 - z * z).sqrt();
            let b = 0.35 + 0.65 * sky_hash(i * 4 + 2);
            CatalogStar {
                id: i as usize,
                direction: Vec3::new(r * a.cos(), z, r * a.sin()),
                color: to_rgb8(Vec3::new(b, b, b * (0.9 + 0.1 * sky_hash(i * 4 + 3)))),
            }
        })
        .collect()
}

/// Vista-proyeccion del firmamento: la vista sin traslacion (solo rota), como `draw_stars`.
pub fn sky_view_projection(camera: &Camera) -> Mat4 {
    let mut view = camera.view();
    for r in 0..3 {
        view.m[r][3] = 0.0;
    }
    camera.projection() * view
}

/// Pixel que `draw_stars` intentaria escribir para `direction` (mismas operaciones y el mismo
/// truncamiento `as isize`), o `None` si queda detras de la camara o fuera del cuadro.
pub fn star_pixel(
    sky_vp: &Mat4,
    direction: Vec3,
    width: usize,
    height: usize,
) -> Option<(isize, isize)> {
    let c = sky_vp.transform(direction.extend(1.0));
    if c.w <= 0.0 {
        return None;
    }
    let (x, y) = (
        (c.x / c.w * 0.5 + 0.5) * width as f64,
        (0.5 - c.y / c.w * 0.5) * height as f64,
    );
    let (x, y) = (x as isize, y as isize);
    (x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height).then_some((x, y))
}

/// Ranking FIJO de aparicion: `ranking[k]` es el ID con rank `k`. Muestreo del punto mas lejano
/// sobre las direcciones del catalogo, desde el ID 0 (empates por ID): cada nueva observadora es
/// la estrella mas alejada de las ya elegidas, asi las primeras quedan repartidas por la esfera y
/// cualquier encuadre ve aproximadamente la misma fraccion. No depende de la camara ni del cuadro.
pub fn observer_ranking() -> Vec<usize> {
    let dirs: Vec<Vec3> = star_catalog().iter().map(|s| s.direction).collect();
    let mut nearest = vec![f64::NEG_INFINITY; STAR_COUNT];
    let mut taken = vec![false; STAR_COUNT];
    let mut ranking = Vec::with_capacity(STAR_COUNT);
    let mut current = 0;
    loop {
        taken[current] = true;
        ranking.push(current);
        if ranking.len() == STAR_COUNT {
            return ranking;
        }
        for (i, d) in dirs.iter().enumerate() {
            if !taken[i] {
                nearest[i] = nearest[i].max(d.dot(dirs[current]));
            }
        }
        // El mas lejano: el menor coseno a su elegida mas cercana.
        current = (0..STAR_COUNT)
            .filter(|&i| !taken[i])
            .min_by(|&a, &b| nearest[a].total_cmp(&nearest[b]).then(a.cmp(&b)))
            .unwrap_or(0);
    }
}

/// Hermite cubica monotona (Fritsch-Butland) por `THRESHOLD_KNOTS`; `r` en `[0, 1]`.
fn threshold_at(r: f64) -> f64 {
    let k = &THRESHOLD_KNOTS;
    let n = k.len();
    let h: Vec<f64> = (0..n - 1).map(|i| k[i + 1].0 - k[i].0).collect();
    let d: Vec<f64> = (0..n - 1).map(|i| (k[i + 1].1 - k[i].1) / h[i]).collect();
    let mut m = vec![0.0; n];
    (m[0], m[n - 1]) = (d[0], d[n - 2]);
    for i in 1..n - 1 {
        if d[i - 1] * d[i] > 0.0 {
            let (w1, w2) = (2.0 * h[i] + h[i - 1], h[i] + 2.0 * h[i - 1]);
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
        }
    }
    let i = (0..n - 1).find(|&i| r <= k[i + 1].0).unwrap_or(n - 2);
    let t = (r - k[i].0) / h[i];
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * k[i].1
        + (t3 - 2.0 * t2 + t) * h[i] * m[i]
        + (-2.0 * t3 + 3.0 * t2) * k[i + 1].1
        + (t3 - t2) * h[i] * m[i + 1]
}

/// Umbral de aparicion de cada ID (indexado por ID), estrictamente por debajo del arranque.
pub fn observer_thresholds() -> Vec<f64> {
    let mut thresholds = vec![0.0; STAR_COUNT];
    for (rank, id) in observer_ranking().into_iter().enumerate() {
        thresholds[id] = threshold_at((rank as f64 + 0.5) / STAR_COUNT as f64);
    }
    thresholds
}

fn valid_oxygen(oxygen01: f64) -> Result<f64, String> {
    if (0.0..=1.0).contains(&oxygen01) {
        Ok(oxygen01)
    } else {
        Err(format!(
            "O2 de las estrellas observadoras fuera de [0, 1]: {oxygen01}"
        ))
    }
}

fn smoothstep01(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Peso de observadora de cada ID (indexado por ID) en `[0, 1]`: 0 es la estrella de siempre y 1
/// el ojo abierto del todo. Acumulativo al bajar el O2 y reversible al subir (funcion pura del O2).
pub fn observer_weights(oxygen01: f64) -> Result<Vec<f64>, String> {
    let oxygen01 = valid_oxygen(oxygen01)?;
    Ok(observer_thresholds()
        .into_iter()
        .map(|t| smoothstep01((t - oxygen01) / OBSERVER_FADE_OXYGEN))
        .collect())
}

/// Densidad global: peso medio de las 700 (0 sin observadoras, 1 todas abiertas).
pub fn observer_density(oxygen01: f64) -> Result<f64, String> {
    let w = observer_weights(oxygen01)?;
    Ok(w.iter().sum::<f64>() / w.len() as f64)
}

/// Profundidad de sondeo del cielo: el mayor `f64` menor que 1. Las estrellas se escriben a
/// profundidad 1 y el fondo queda en infinito; `depth_passes(x, y, SKY_DEPTH_PROBE)` es verdadero
/// solo donde no hay geometria delante (profundidad guardada >= 1).
pub const SKY_DEPTH_PROBE: f64 = 1.0 - f64::EPSILON / 2.0;

/// Un ojo dibujado.
#[derive(Clone, Debug, PartialEq)]
pub struct ObserverEye {
    pub id: usize,
    pub rank: usize,
    pub pixel: (isize, isize),
    pub weight: f64,
    /// Desplazamiento de la pupila en pixeles (hacia la nave proyectada; 0 sin proyeccion).
    pub pupil_offset: [f64; 2],
    pub pixels_changed: usize,
}

/// Metadatos del cuadro: politica (pesos) frente a lo que de verdad se dibujo.
#[derive(Clone, Debug, PartialEq)]
pub struct WatchingStarsReport {
    pub oxygen01: f64,
    pub density: f64,
    /// IDs con peso > 0 en toda la esfera.
    pub active_ids: Vec<usize>,
    /// Activos que caen dentro del cuadro y son la estrella visible de su pixel.
    pub projected_ids: Vec<usize>,
    /// Proyectados con geometria delante de su centro: sin ojo.
    pub occluded_ids: Vec<usize>,
    /// Activos cuyo pixel ya lo ocupaba una estrella anterior en `draw_stars`: sin ojo.
    pub shadowed_ids: Vec<usize>,
    pub eyes: Vec<ObserverEye>,
    /// Nave proyectada en pixeles (sin acotar), o `None` sin proyeccion valida.
    pub ship_focus_px: Option<[f64; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WatchingStars {
    pub pixels: Vec<u32>,
    pub report: WatchingStarsReport,
}

/// Ojo diminuto («Pupilas de alfiler»), en pixeles y centrado en el pixel de su estrella: almendra
/// horizontal de 4.4 px de semiancho y 2.3 px de semialto (cabe en 9x5), con supermuestreo 4x4.
/// Es un tamano fijo en pixeles, como la estrella de 1 px: lejano, no una cara.
pub const EYE_HALF_WIDTH_PX: f64 = 4.4;
pub const EYE_HALF_HEIGHT_PX: f64 = 2.3;
/// Apertura del parpado con peso 0, como fraccion del semialto: al crecer el peso el ojo se abre
/// (de rendija a almendra) a la vez que gana opacidad. Estatico para un O2 dado: sin parpadeo.
pub const EYE_CLOSED_APERTURE: f64 = 0.35;
pub const IRIS_RADIUS_PX: f64 = 1.6;
pub const PUPIL_RADIUS_PX: f64 = 0.8;
/// Desplazamiento maximo de iris y pupila hacia la nave; se reduce si la nave esta a < 12 px.
pub const PUPIL_SHIFT_PX: f64 = 1.3;
const PUPIL_NEAR_PX: f64 = 12.0;
/// Esclerotica: el color de la propia estrella, algo apagado (los ojos tenues son los lejanos).
pub const SCLERA_GAIN: f64 = 0.8;
/// Iris ambar palido (0-1), escalado por el brillo de su estrella; pupila casi negra.
pub const IRIS_COLOR: [f64; 3] = [0.86, 0.62, 0.30];
pub const PUPIL_COLOR: u32 = 0x06_0308;
const SUBSAMPLES: usize = 4;
const EYE_BOX: (isize, isize) = (4, 2);

fn channels(p: u32) -> [f64; 3] {
    [16, 8, 0].map(|s| f64::from((p >> s) & 0xff))
}

fn pack(c: [f64; 3]) -> u32 {
    let ch = |v: f64| (v.clamp(0.0, 255.0) + 0.5) as u32;
    (ch(c[0]) << 16) | (ch(c[1]) << 8) | ch(c[2])
}

fn finite_matrix(m: &Mat4) -> bool {
    m.m.iter().flatten().all(|v| v.is_finite())
}

/// Todo lo observable se valida antes de reservar memoria o consultar la profundidad. La
/// profundidad es privada de `Framebuffer`: un cuadro construido con `Framebuffer::new` la tiene del
/// mismo tamano que `pixels`; si alguien cambiara `width`/`height`/`pixels` (campos publicos)
/// conservando el producto, esa corrupcion privada no es verificable desde aqui.
fn validate(
    frame: &Framebuffer,
    camera: &Camera,
    ship_position: Vec3,
    oxygen01: f64,
) -> Result<(), String> {
    valid_oxygen(oxygen01)?;
    if !ship_position.is_finite() {
        return Err("posicion de la nave no finita".into());
    }
    if !(camera.position.is_finite() && camera.target.is_finite() && camera.up.is_finite()) {
        return Err("camara con posicion, objetivo o up no finitos".into());
    }
    let forward = camera.target - camera.position;
    if !(forward.length() > 0.0 && camera.up.length() > 0.0) {
        return Err("camara sin direccion de vista o sin up".into());
    }
    if !(finite_matrix(&camera.view_projection()) && finite_matrix(&sky_view_projection(camera))) {
        return Err("camara con matrices no finitas".into());
    }
    let count = frame
        .width
        .checked_mul(frame.height)
        .filter(|&n| n > 0 && n <= MAX_PIXELS)
        .ok_or("dimensiones de cuadro invalidas o demasiado grandes")?;
    if frame.pixels.len() != count {
        return Err(format!(
            "el cuadro tiene {} pixeles para {}x{}",
            frame.pixels.len(),
            frame.width,
            frame.height
        ));
    }
    Ok(())
}

/// La nave proyectada con la camara REAL (con traslacion), en pixeles y sin acotar; `None` si
/// queda detras de la camara (`w <= 0`) o la proyeccion no es finita: entonces las pupilas quedan
/// centradas (miran al jugador).
fn ship_focus_px(camera: &Camera, ship: Vec3, width: usize, height: usize) -> Option<[f64; 2]> {
    let c = camera.view_projection().transform(ship.extend(1.0));
    let p = [
        (c.x / c.w * 0.5 + 0.5) * width as f64,
        (0.5 - c.y / c.w * 0.5) * height as f64,
    ];
    (c.w > 0.0 && p.iter().all(|v| v.is_finite())).then_some(p)
}

fn pupil_offset(pixel: (isize, isize), focus: Option<[f64; 2]>) -> [f64; 2] {
    let Some(f) = focus else {
        return [0.0, 0.0];
    };
    let v = [f[0] - pixel.0 as f64 - 0.5, f[1] - pixel.1 as f64 - 0.5];
    let len = v[0].hypot(v[1]);
    if len < 1e-9 {
        return [0.0, 0.0];
    }
    let shift = PUPIL_SHIFT_PX * (len / PUPIL_NEAR_PX).min(1.0) / len;
    [v[0] * shift, v[1] * shift]
}

/// Dibuja un ojo sobre `out` solo en pixeles de cielo de `frame` (profundidad >= 1); devuelve los
/// pixeles que cambiaron.
fn draw_eye(
    frame: &Framebuffer,
    out: &mut [u32],
    pixel: (isize, isize),
    star_color: u32,
    weight: f64,
    offset: [f64; 2],
) -> usize {
    let aperture =
        EYE_HALF_HEIGHT_PX * (EYE_CLOSED_APERTURE + (1.0 - EYE_CLOSED_APERTURE) * weight);
    let star = channels(star_color);
    let brightness = star[1] / 255.0;
    let sclera = star.map(|c| c * SCLERA_GAIN);
    let iris = IRIS_COLOR.map(|c| c * 255.0 * brightness);
    let pupil = channels(PUPIL_COLOR);
    let mut changed = 0;
    for dy in -EYE_BOX.1..=EYE_BOX.1 {
        for dx in -EYE_BOX.0..=EYE_BOX.0 {
            let (x, y) = (pixel.0 + dx, pixel.1 + dy);
            if !frame.depth_passes(x, y, SKY_DEPTH_PROBE) {
                continue; // fuera del cuadro o con geometria delante
            }
            let (mut inside, mut sum) = (0usize, [0.0; 3]);
            for sy in 0..SUBSAMPLES {
                for sx in 0..SUBSAMPLES {
                    let u = dx as f64 + (sx as f64 + 0.5) / SUBSAMPLES as f64 - 0.5;
                    let v = dy as f64 + (sy as f64 + 0.5) / SUBSAMPLES as f64 - 0.5;
                    let lid = aperture * (1.0 - (u / EYE_HALF_WIDTH_PX).powi(2));
                    if lid <= 0.0 || v.abs() > lid {
                        continue;
                    }
                    let r = (u - offset[0]).hypot(v - offset[1]);
                    let c = if r <= PUPIL_RADIUS_PX {
                        pupil
                    } else if r <= IRIS_RADIUS_PX {
                        iris
                    } else {
                        sclera
                    };
                    inside += 1;
                    sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
                }
            }
            if inside == 0 {
                continue;
            }
            let alpha = weight * inside as f64 / (SUBSAMPLES * SUBSAMPLES) as f64;
            let i = y as usize * frame.width + x as usize;
            let base = channels(out[i]);
            let next = pack(std::array::from_fn(|k| {
                base[k] + (sum[k] / inside as f64 - base[k]) * alpha
            }));
            if next != out[i] {
                out[i] = next;
                changed += 1;
            }
        }
    }
    changed
}

/// Estrellas observadoras sobre el cuadro YA dibujado (`frame`, con su profundidad), con la camara
/// del cuadro, la posicion real de la nave y el O2. Devuelve pixeles nuevos y metadatos; `frame`
/// (pixeles y profundidad) no cambia. Con O2 >= `OBSERVER_ONSET_OXYGEN` los pixeles son EXACTOS.
/// - Mismas 700 estrellas de `draw_stars` (mismo pixel, orden y color); el ojo de una estrella solo
///   se dibuja si es la estrella visible de su pixel y no hay geometria delante de su centro.
/// - Cada pixel del ojo se pinta solo donde la profundidad es >= 1 (cielo): nunca sobre planetas,
///   la nave u otra geometria.
pub fn render_watching_stars(
    frame: &Framebuffer,
    camera: &Camera,
    ship_position: Vec3,
    oxygen01: f64,
) -> Result<WatchingStars, String> {
    validate(frame, camera, ship_position, oxygen01)?;
    let weights = observer_weights(oxygen01)?;
    let ranking = observer_ranking();
    let mut rank_of = vec![0; STAR_COUNT];
    for (rank, &id) in ranking.iter().enumerate() {
        rank_of[id] = rank;
    }
    let (width, height) = (frame.width, frame.height);
    let sky_vp = sky_view_projection(camera);
    let catalog = star_catalog();
    // Como en `draw_stars`: la primera estrella que llega a un pixel es la que se ve.
    let mut owner = std::collections::HashMap::new();
    let pixels_of: Vec<Option<(isize, isize)>> = catalog
        .iter()
        .map(|s| {
            let p = star_pixel(&sky_vp, s.direction, width, height);
            if let Some(p) = p {
                owner.entry(p).or_insert(s.id);
            }
            p
        })
        .collect();
    let focus = ship_focus_px(camera, ship_position, width, height);
    let mut pixels = frame.pixels.clone();
    let mut report = WatchingStarsReport {
        oxygen01,
        density: weights.iter().sum::<f64>() / STAR_COUNT as f64,
        active_ids: Vec::new(),
        projected_ids: Vec::new(),
        occluded_ids: Vec::new(),
        shadowed_ids: Vec::new(),
        eyes: Vec::new(),
        ship_focus_px: focus,
    };
    for star in &catalog {
        let weight = weights[star.id];
        if weight <= 0.0 {
            continue;
        }
        report.active_ids.push(star.id);
        let Some(pixel) = pixels_of[star.id] else {
            continue;
        };
        if owner.get(&pixel) != Some(&star.id) {
            report.shadowed_ids.push(star.id);
            continue;
        }
        report.projected_ids.push(star.id);
        if !frame.depth_passes(pixel.0, pixel.1, SKY_DEPTH_PROBE) {
            report.occluded_ids.push(star.id);
            continue;
        }
        let offset = pupil_offset(pixel, focus);
        let changed = draw_eye(frame, &mut pixels, pixel, star.color, weight, offset);
        report.eyes.push(ObserverEye {
            id: star.id,
            rank: rank_of[star.id],
            pixel,
            weight,
            pupil_offset: offset,
            pixels_changed: changed,
        });
    }
    Ok(WatchingStars { pixels, report })
}
