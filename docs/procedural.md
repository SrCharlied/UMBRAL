# Distribución procedural sobre esfera (UMB-304)

**Estado:** primera implementación, propietaria Persona 3. Código en `src/visual/procedural_surface.rs`, pruebas en `tests/procedural.rs`, vista técnica en `examples/procedural_surface.rs`. **No está integrada en la escena del juego**: ningún planeta, prop, shader, cámara ni control cambia con este trabajo. La orientación completa de props (UMB-305) no está hecha.

## Qué produce

```rust
pub fn distribute(settings: &DistributionSettings) -> Result<Vec<SurfaceSample>, String>;
pub fn density(seed: u64, noise_scale: f64, point: Vec3) -> Result<f64, String>;
```

Cada `SurfaceSample` tiene `position`, `normal`, `density` y `attempt`:

- **Espacio local del planeta:** esfera de **radio 1** centrada en el origen, con las convenciones de `math` (mano derecha, Y arriba). No hay órbita, rotación, tiempo ni posición de mundo horneados.
- **Normal radial unitaria**, igual a `position` mientras la superficie sea la esfera. Es suficiente para colocar marcadores; no define un giro alrededor de la normal (eso es UMB-305).
- **Orden:** creciente por `attempt`, el índice del candidato que generó la muestra.

**Uso previsto al integrar (no implementado ni verificado):** escalar por el radio del planeta y aplicar la misma `Transform` del planeta. Así las muestras seguirían su rotación y órbita sin recalcularse. Que los props queden pegados durante la rotación **no se ha comprobado**; se probará con UMB-305.

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

## Pendiente

- **UMB-305:** orientación completa (base tangente estable, giro alrededor de la normal) y prueba de que los props siguen la rotación del planeta.
- **Integración:** qué planetas usan qué semilla y parámetros, radio real y densidad visual. Es decisión de Persona 3 con revisión de Persona 1 para el renderer.
- **Rendimiento:** no medido dentro de la escena. Generar una vez por planeta, no por cuadro.
- **Posibles mejoras, solo si hacen falta:** varias octavas de ruido, o una retícula espacial para el espaciado si se sube `MAX_ATTEMPTS`.
