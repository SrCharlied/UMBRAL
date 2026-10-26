# Distribución y anclaje procedural sobre esfera (UMB-304 y UMB-305)

**Estado:** primera implementación, propietaria Persona 3. Código en `src/visual/procedural_surface.rs`, pruebas en `tests/procedural.rs`, vista técnica en `examples/procedural_surface.rs`. **No está integrada en la escena del juego**: ningún planeta, prop, shader, cámara ni control cambia con este trabajo. La orientación y el anclaje de props (UMB-305) están en la sección «Orientación y anclaje (UMB-305)», también sin integrar.

## Qué produce

```rust
pub fn distribute(settings: &DistributionSettings) -> Result<Vec<SurfaceSample>, String>;
pub fn density(seed: u64, noise_scale: f64, point: Vec3) -> Result<f64, String>;
```

Cada `SurfaceSample` tiene `position`, `normal`, `density` y `attempt`:

- **Espacio local del planeta:** esfera de **radio 1** centrada en el origen, con las convenciones de `math` (mano derecha, Y arriba). No hay órbita, rotación, tiempo ni posición de mundo horneados.
- **Normal radial unitaria**, igual a `position` mientras la superficie sea la esfera. Es suficiente para colocar marcadores; el giro alrededor de la normal lo añade `SurfaceAnchor` (UMB-305).
- **Orden:** creciente por `attempt`, el índice del candidato que generó la muestra.

**Uso con props:** crear un `SurfaceAnchor` por muestra una sola vez y componerlo con la `Transform` del planeta en cada cuadro (sección UMB-305). Que el anclaje siga la órbita y la rotación de un `CelestialBody` está probado en `tests/surface_placement.rs`; en la escena del juego todavía no se usa.

## Algoritmo

1. **Candidatos.** El candidato `i` depende solo de `(seed, i)`, mediante un hash por contador construido con el finalizador de SplitMix64 (propio, sin dependencias). Se muestrea **uniforme en área** con el teorema de Arquímedes: `y = 1 − 2u` y longitud `φ = 2πv`. No se acumulan muestras en los polos, como pasaría con latitud uniforme.
2. **Densidad.** **Ruido de valor 3D**: valores pseudoaleatorios en los nodos de una retícula entera, combinados por interpolación trilineal con suavizado cúbico `t²(3 − 2t)`. **No es ruido de Perlin**, porque no usa gradientes. Se evalúa en `position · noise_scale` en 3D, así que no tiene costura en el meridiano ni deformación polar. Es una sola octava. Los nodos valen en `[0, 1)`, pero la interpolación en coma flotante no garantiza quedar estrictamente por debajo de 1, así que el rango garantizado es **`[0, 1]`** (un 1.0 exacto no se ha observado). Usa un flujo del hash distinto al de los candidatos.
3. **Umbral.** Se acepta el candidato si `density ≥ threshold`.
4. **Espaciado mínimo opcional.** Se rechaza un candidato si queda a una distancia cordal menor que `min_spacing` de alguna muestra **ya aceptada**. Es un recorrido greedy en orden de intento: **no** es un muestreo de disco de Poisson máximo.

## Parámetros, unidades y límites

| Campo | Tipo | Rango aceptado | Significado |
|---|---|---|---|
| `seed` | `u64` | cualquiera | Semilla; misma semilla y parámetros, misma salida |
| `attempts` | `u32` | `0..=4096` (`MAX_ATTEMPTS`) | Candidatos evaluados. `0` devuelve una lista vacía válida |
| `noise_scale` | `f64` | `(0, 1024]` (`MAX_NOISE_SCALE`), finito | Celdas de ruido por unidad de radio; más alto, manchas más pequeñas |
| `threshold` | `f64` | `[0, 1]`, finito | Densidad mínima. `0` acepta todo; `1` en la práctica no acepta nada (solo una densidad redondeada a 1.0 pasaría) |
| `min_spacing` | `f64` | `[0, 2]`, finito | Cuerda mínima sobre la esfera unidad; `0` lo desactiva. Ángulo equivalente: `θ = 2·asin(c/2)`. Para un planeta de radio `R`, la distancia real es `c·R` |

- **Validación previa.** Todo se valida **antes** de reservar memoria o evaluar candidatos. NaN, infinitos o valores fuera de rango devuelven `Err` con el nombre del campo.
- **Presupuesto.** El espaciado compara cada candidato con las muestras aceptadas: `O(attempts · aceptadas)`, con peor caso ≈ 8.4 M comparaciones con 4096 intentos. La salida nunca supera `attempts`, así que no hace falta otro tope.
- **Dominio de `density`.** La función pública devuelve `Err` en vez de un valor inventado si `noise_scale` está fuera de `(0, 1024]` o no es finito, si `point` tiene alguna componente no finita, o si `|point_i · noise_scale| > 2²⁰` (`MAX_NOISE_COORD`). Dentro de ese dominio la conversión a la retícula entera es exacta y quedan unos 32 bits de parte fraccionaria; fuera de él no se promete continuidad. Admite puntos fuera de la esfera unidad (por ejemplo, posiciones de fragmento en espacio local) mientras estén en el dominio. `distribute` usa la misma función interna sin repetir la validación, porque sus parámetros ya se validaron y sus puntos son unitarios.

## Garantías (cubiertas por `tests/procedural.rs`)

- **Reproducibilidad:** misma semilla y parámetros dan la misma salida en el mismo orden; otra semilla da otra salida.
- **Geometría:** posiciones y normales finitas y unitarias, y la normal es radial.
- **Sin sesgo polar:** con umbral 0, la mitad de las muestras cae en `|y| > 0.5` (latitud uniforme daría dos tercios, porque `|latitud| > 30°` es 60 de 90 grados) y cada hemisferio recibe la mitad, con tolerancia ±0.04 en 4096 muestras.
- **Densidad:** acotada a `[0, 1]`, reproducible, continua (un desplazamiento de 1e-6 la cambia < 1e-3) y distinta al cambiar la semilla o `noise_scale`. Con entrada directa rechaza escala inválida, puntos NaN o infinitos y coordenadas escaladas fuera de `±2²⁰`, incluido el caso `splat(f64::MAX)`; acepta los bordes exactos del dominio.
- **Umbral:** con `min_spacing = 0`, el resultado es exactamente el filtro de los candidatos, y subir el umbral conserva un **subconjunto**. Con `min_spacing > 0` esto deja de garantizarse, porque el resultado greedy depende de qué se aceptó antes.
- **Espaciado:** ningún par aceptado queda más cerca que `min_spacing`, y cada candidato descartado choca con una muestra aceptada antes que él.
- **Límites:** NaN, infinitos, valores fuera de rango, `attempts = 4097` y `u32::MAX` se rechazan. Los extremos documentados (0, `MAX_ATTEMPTS`, `MAX_NOISE_SCALE`, umbral 1, espaciado 2) se aceptan.

La distribución resultante **no es uniforme**: se agrupa donde el ruido supera el umbral. Por eso los conteos norte/sur de una semilla concreta pueden diferir bastante.

## Vista técnica

```bash
cargo run --release --example procedural_surface -- --seed 24531
cargo run --release --example procedural_surface -- --seed 24532
```

Escribe `renders/smoke/procedural_seed_<semilla>.png` a la resolución por defecto del juego (1280×720), salvo que se use `--output`. Más opciones: `--attempts N --noise-scale X --threshold X --min-spacing X`. Los valores por defecto del ejemplo son `attempts 2048`, `noise_scale 2.5`, `threshold 0.55` y `min_spacing 0.08`; **son de demostración, no parámetros aprobados de ningún planeta**.

- La imagen muestra una esfera gris (`LitPlanet` de Persona 1) y un marcador esférico por muestra.
- Cada marcador tiene radio 0.025 y su centro está elevado un radio sobre la superficie. Por eso sobresale entero; en el limbo se ve salir de la silueta.
- El color va de cian (densidad igual al umbral) a naranja (densidad 1).
- Los marcadores de la cara trasera los oculta el z-buffer.

Es **una vista técnica y no arte final**: ni los marcadores son props ni la esfera es un planeta aprobado. Imprime estadísticas: aceptadas, rango de densidad, cuerda mínima real, norte/sur y píxeles escritos por los marcadores.

## Orientación y anclaje (UMB-305)

**Estado:** API pura y vista técnica; propietaria Persona 3. Código en `src/visual/surface_placement.rs`, pruebas en `tests/surface_placement.rs`, vista en `examples/surface_placement.rs`. No hay integración en la escena, en la cámara ni en el pipeline.

```rust
pub struct PropPlacement { pub twist: f64, pub elevation: f64, pub scale: f64 }
impl SurfaceAnchor {
    pub fn new(normal: Vec3, placement: PropPlacement) -> Result<Self, String>;
    pub fn local_matrix(&self) -> Mat4;                                             // espacio del planeta, radio 1
    pub fn world_matrix(&self, planet: &Transform) -> Result<Mat4, String>;         // pose exacta
    pub fn world_transform(&self, planet: &Transform) -> Result<Transform, String>; // para draw_model
}
```

### Convención del prop

Se asume un modelo **con el pivote en la base y +Y hacia arriba**. Su altura en unidades del modelo multiplicada por `scale` da la altura en radios del planeta.

### Composición

`M = T_planeta · R_planeta · S_radio · T_anclaje · R_alineación · R_giro · S_prop`

- `T_anclaje` lleva el pivote a `n · (1 + elevation)` sobre la esfera unidad local.
- `R_alineación` lleva +Y del modelo a la normal `n`.
- `R_giro = rotation_y(twist)` gira alrededor de esa normal.
- `S_prop = scale`.

Los tres factores de la derecha forman el **anclaje local persistente** (`SurfaceAnchor`). Se calcula una vez desde la normal *local* de la muestra, y cada cuadro solo cambia la `Transform` del planeta. Nunca se recalcula la base desde la normal en el mundo, así que no hay saltos de orientación mientras el planeta gira u orbita.

### Base tangente

Con `twist = 0`:
- `Y' = n`.
- `Z' = normalizar(Y × n)`, horizontal (perpendicular al eje del planeta).
- `X' = n × Z'`, la proyección del norte sobre el plano tangente.

Es determinista y de mano derecha. Cuando `|Y × n| < 1e-6` (a menos de ~1e-6 rad de un polo) se usa `Z × n` como respaldo. **No existe una base tangente continua en toda la esfera** (teorema de la bola peluda): hay un salto de orientación en ese casquete diminuto, y además la dirección «este» gira rápido cerca de los polos. Es una convención, no una garantía de continuidad global.

### Unidades y dominios (se valida cada entrada y la salida)

| Entrada | Rango | Unidad |
|---|---|---|
| `normal` | Finita y unitaria (longitud a menos de 1e-9 de 1); se rechazan cero, NaN, infinitos y vectores no unitarios | Espacio local del planeta (p. ej. `SurfaceSample::normal`) |
| `twist` | Finito | Radianes alrededor de la normal |
| `elevation` | `[-0.25, 1.0]` (`MIN_ELEVATION`, `MAX_ELEVATION`) | Radios del planeta; negativo hunde el pivote |
| `scale` | f64 **normal** en `[f64::MIN_POSITIVE, 1.0]` (`MAX_PROP_SCALE`); se rechazan cero, `-0.0` y subnormales | Radios del planeta por unidad del modelo |
| `planet.scale` | Uniforme (`x == y == z`), finita y `> 0` | Radio del planeta. Se rechazan elipsoides y reflexiones |
| `planet.position`, `planet.rotation` | Finitas | Mundo / radianes (Euler de `Transform`) |
| `radio · scale` (escala del prop en el mundo) | `[1e-150, 1e150]` (`MIN_WORLD_SCALE`, `MAX_WORLD_SCALE`) | Se comprueba en `world_matrix` y en `world_transform` |

**Por qué existe el dominio de escala en el mundo:**
- **Subdesbordamiento.** Un producto que se va a cero, como `1e-200 · 1e-200`, es finito pero degenerado: la pose se aplasta en un punto.
- **Inversión en el pipeline.** `Transform::transform_normal` normaliza `n / s`, así que `(1/s)²` tiene que ser un f64 normal. Con escala 1e-300, `n / s` mide 1e300 y su cuadrado desborda, de modo que `normalized()` devuelve cero. Con escalas de ~1e200, ese cuadrado subdesborda.
- **El rango `[1e-150, 1e150]`** deja `(1/s)²` en `[1e-300, 1e300]`. Ambos extremos están probados: matriz no degenerada y normales unitarias en el pipeline.

**Desbordamiento de la pose.** El límite de `radio · scale` controla el tamaño del prop en el mundo, no el radio del planeta ni el desplazamiento de su pivote. Un radio enorme con un prop muy pequeño puede dar una escala mundial válida y aun así desbordar `radio · (1 + elevation)`. Por eso la comprobación de salida finita es necesaria y devuelve `Err` en ambas APIs; hay una prueba explícita con radio `f64::MAX`, escala de prop `1e-200` y elevación máxima. También se prueban casos finitos con posición de planeta `±f64::MAX` y radio acotado: no significan que toda combinación de entradas finitas sea representable.

### `world_transform`: adaptador exacto a `Transform`

`draw_model` solo acepta `Transform` (ángulos de Euler). Con escala de planeta uniforme, la composición se reduce exactamente a `T' · R' · S(radio · scale)`.

**Los ángulos de `R'` los calcula un helper privado, `euler_yxz`, y no `Mat4::to_euler`.** Es la adaptación precisa a la convención de `Mat4::from_euler` (`Ry · Rx · Rz`) con el método de Mike Day:
1. Lee la guiñada con `atan2(m02, m22)`.
2. La retira (`Ry(−guiñada) · R`).
3. Lee el cabeceo y el alabeo del resto (`Rx · Rz`), que está bien condicionado.

Cerca del gimbal la guiñada pierde precisión, pero esa imprecisión queda absorbida en la matriz residual, así que el resultado es exacto hasta el redondeo **en toda la esfera**, sin umbral ni rama especial. Medido: el error máximo de `world_transform` frente a `world_matrix` alrededor del gimbal es **1.0e-15**. Los casos cubren δ ∈ {0, ±1e-12, ±1e-9, ±1e-6, ±1e-4, ±1e-3, ±1.4e-3, ±2e-3}, ambos signos de `m12` y un planeta inclinado y trasladado.

**Por qué no `to_euler`.** `Mat4::to_euler` (Persona 1) entra en su rama de cardán con `|m12| ≥ 0.999999` y ahí fija el cabeceo en ±π/2. Usándola, el adaptador tenía un error evitable de hasta ~1.4e-3 rad: el repro de revisión, normal X con `twist = π/2 − 0.001`, daba 1e-3. Ahora ese comportamiento solo se **caracteriza** en un test diagnóstico separado, que no participa en el contrato de `world_transform`:

| Distancia al gimbal (ambos signos) | Error de `to_euler` → `from_euler` |
|---|---|
| δ = 1e-4 | 9.2e-5 |
| δ = 1e-3 | 9.2e-4 |
| δ = 1.4e-3 | 1.29e-3 |
| δ = 2e-3 (fuera de la banda) | 1.6e-14 |

`world_matrix` sigue siendo la pose de referencia, y `world_transform` la reproduce con tolerancia uniforme de 1e-9 en todas las pruebas. **No se modificó `math`.** La propuesta de unificar el helper con `to_euler` está en Pendiente.

### Garantías (cubiertas por `tests/surface_placement.rs`)

- **Base local:**
  - Y local igual a la normal en ±X, ±Y, ±Z, cerca de los polos (a ambos lados del umbral de respaldo) y en direcciones arbitrarias.
  - Base ortonormal de mano derecha; pie sobre la esfera.
  - Misma entrada, misma matriz.
- **Convención con `twist = 0`:** Z local horizontal y X local hacia el norte.
- **Giro:** conserva la normal y el pie, y rota la tangente exactamente `X' = cos t·X₀ − sin t·Z₀`; un giro de 2π es la identidad.
- **Unidades:** elevación y escala en radios; el pie, la punta y la base tangente del modelo caen donde deben.
- **`CelestialBody` real (Antena):** con anclajes creados una vez desde `distribute` y evaluados en t = 0, 7.5 y 31 s:
  - El contacto está a `r(1 + e)` del centro, en la posición rotada de la muestra.
  - El eje Y es la radial del mundo.
  - Cualquier punto del prop, expresado en el marco del planeta, no cambia con el tiempo.
- **`world_transform` frente a `world_matrix`:** error < 1e-9 (por elemento y por norma de columna) en:
  - polos, ecuador y planetas trasladados, escalados y rotados arbitrariamente;
  - `CelestialBody` en dos tiempos;
  - el gimbal exacto y todo su entorno, con ambos signos.
- **Validación:**
  - Normales y colocaciones inválidas, escalas de prop subnormales, planetas no uniformes, reflejados, de escala cero o no finitos, y escalas en el mundo fuera de `[1e-150, 1e150]` (subdesbordamiento incluido) devuelven `Err`.
  - Los extremos aceptados dan poses finitas y normales del pipeline unitarias: escala en el mundo 1e-150 y 1e150, posición `±f64::MAX`, radio 1e6, elevación y escala en sus límites, giro 1e6.
- **Diagnóstico de `Mat4::to_euler`:** caracteriza su banda actual. Fallará si Persona 1 la corrige, y entonces hay que actualizarlo junto con esta doc.

### Vista técnica

```bash
cargo run --release --example surface_placement -- --angle 0
cargo run --release --example surface_placement -- --angle 1
```

Escribe `renders/smoke/surface_placement_<angulo>.png` (1280×720), salvo que se use `--output`. También admite `--seed N` (por defecto 24531).

- **Planeta de la escena:** una esfera de radio 1.6 trasladada a `(3, −1, −2)` que gira `--angle` rad en Y.
- **Marcadores de diagnóstico:** obelisco claro con el pie en Y = 0, cuña naranja hacia +Z local y aleta cian hacia +X local.
  - Polos: escala 0.3.
  - Doce en el ecuador con giro 0: escala 0.2.
  - Muestras de `distribute` (umbral 0.55, espaciado 0.35): escala 0.14, con giro `attempt · ángulo áureo`, reproducible.
- **Lo que imprime:** el error máximo de contacto, del eje vertical y entre `Transform` y matriz.

Las dos capturas usan la misma semilla y los mismos anclajes. Lo único que cambia es la `Transform` del planeta. **Es una vista técnica:** los marcadores no son props definitivos ni la esfera es un planeta aprobado.

## Pendiente

- **Unificar con Persona 1 (propuesta, no aplicada):** `world_transform` ya no depende de la banda de `to_euler`, porque usa el helper privado `euler_yxz`. Si Persona 1 adopta el mismo método en `Mat4::to_euler` (retirar la guiñada y leer cabeceo y alabeo del resto), el helper puede eliminarse y el diagnóstico de banda debe actualizarse. Lo decide su propietaria.
- **Integración en la escena:** asignar props por planeta y elegir giros, elevaciones y escalas. Los props reales y su modelo no forman parte de este trabajo.
- **Integración de la distribución:** qué planetas usan qué semilla y parámetros, radio real y densidad visual. Es decisión de Persona 3 con revisión de Persona 1 para el renderer.
- **Rendimiento:** no medido dentro de la escena. Generar una vez por planeta, no por cuadro.
- **Posibles mejoras, solo si hacen falta:** varias octavas de ruido, o una retícula espacial para el espaciado si se sube `MAX_ATTEMPTS`.
