# Ojo del vacío (UMB-309), preview aislado

**Estado:** variante **02** de la hipótesis **«pupila del vacío»**, **aprobada por Charlie** e integrada opt-in en el juego con `--void-eye` (ver [Integración en el juego](#integración-en-el-juego---void-eye)); integración promovida al proyecto principal y verificada por Veliona, incluidos los lanzadores reales. La comprobación física de los controles queda a cargo de Charlie. Este documento describe también el preview aislado del shader `ShaderKind::VoidEye`.
- **Cambio frente a la 01** (pedido por Charlie): los anillos finos del iris se funden en **una sola corona ancha y continua** sobre la misma banda. La pupila, el centro oscuro, el seguimiento de la nave, el tamaño, los colores y la composición no cambian. La 01 se conserva como referencia en `tests/fixtures/void_v1_shader.rs`.
- **No es** el ojo de la criatura ni la criatura, que están aprobados y congelados y no se tocan.
- **No es** una identidad final de planeta: `SolarSystem::demo` sigue usando `LitPlanet` en todos los planetas; solo `--void-eye` cambia el shader de Vigilia.
- **No añade** consumo de oxígeno, filtro global, HUD ni gameplay.

Código en `src/renderer/shader.rs` (`void_eye`), pruebas en `tests/void_eye.rs` y vista técnica sin ventana en `examples/void_eye_preview.rs`.

## Qué hace

Sobre la esfera existente, la piedra iluminada (`lit_planet`) se ve más oscura (×0.6), con tres elementos:
- **Pupila:** casi negra (oscurece un 96 %, sin emisión), centrada donde la superficie **mira a la nave**.
- **Iris:** la misma banda alrededor de la pupila, llena por **una corona** rojo-ámbar continua (emisión máxima 0.12, la de la 01). Su perfil radial es exactamente la envolvente del iris de la 01: sube tras el borde de la pupila, se mantiene y se apaga hacia `IRIS_RADIUS`, sin franjas ni valles.
- **Movimiento:** la corona **respira**, toda a la vez, en un ciclo de 3 s. El factor `ring` baja de 1 a 0.8 y vuelve; también interviene en la mezcla rojo→ámbar, por lo que ese 20 % describe el factor, no una caída exacta de luminancia o de cada canal final. No se desplaza, no crece hacia el centro ni fuera del iris. La pupila no cambia con el tiempo: solo sigue a la nave.

| Constante | Valor | Uso |
|---|---|---|
| `VOID_STONE` | 0.6 | Fracción de la piedra iluminada fuera de pupila e iris |
| `PUPIL_RADIUS` / `PUPIL_SOFT` | 0.3 / 0.04 rad | Pupila y su borde |
| `IRIS_RADIUS` / `IRIS_SHADE` | 0.75 rad / 0.35 | Borde del iris y su oscurecimiento |
| `RING_PERIOD_SECONDS` / `CORONA_BREATH` | 3 s / 0.2 | Periodo y profundidad del factor `ring` (1→0.8), no porcentaje exacto de luminancia final; la 01 usaba además `RING_SPACING`, ya retirada |
| `IRIS_RED` → `IRIS_AMBER`, `IRIS_GLOW` | (0.35, 0.04, 0.02) → (0.8, 0.3, 0.06), 0.12 | Acento |

Son parámetros de una sola hipótesis, no decisiones aprobadas.

## Política geométrica

El ojo usa solo la **normal** del fragmento y la **dirección del fragmento a la nave**, `ship_position − world_position`. El ángulo entre ambas es 0 en el punto de la esfera que mira a la nave, cualquiera que sea su centro o su radio.
- **No supone** que el cuerpo esté en el origen ni que tenga radio 1. Una prueba traslada la escena entera y otra pone la esfera en (10, −3, 2).
- **El eje y la corona del ojo no usan** `camera_position` como objetivo, ni la UV ni el oxígeno. La piedra base sí conserva el patrón UV y la iluminación de `lit_planet`; el color completo no es independiente de la UV ni de la luz.
- Es una función pura del fragmento, el contexto y el material: sin memoria, IO ni caché.

**Límites:**
- **Nave cercana:** la dirección cambia de un fragmento a otro y la pupila se encoge hacia el punto más cercano. Con la nave lejana es un casquete de radio angular fijo.
- **Nave dentro de la esfera, o con una normal que no corresponde a la posición** (otra malla, normales arbitrarias): el patrón sigue siendo continuo, pero deja de describir «la cara que mira a la nave».
- **Pupila oculta:** si la nave está detrás del cuerpo respecto de la cámara, la pupila queda en la cara oculta y no se ve. Es la consecuencia esperada de mirar a la nave y no a la cámara.

## Entradas adversas

| Entrada | Resultado |
|---|---|
| Nave coincidente con el fragmento, o nave `NaN` / `±Inf` | Ojo **cerrado**: solo la piedra oscura (`lit_planet × 0.6`), finita e igual en todos esos casos |
| Nave finita pero enorme (por ejemplo, 1e300) | La dirección se escala antes de normalizar: la pupila sigue mirándola |
| `elapsed_time` `NaN` / `±Inf` | Como `t = 0` (solo en este shader) |
| Tiempo enorme (`±f64::MAX`) | Finito: la fase se reduce a una fracción de periodo |

**Precondiciones** (como en los demás shaders, que no se sanean): normal no nula y finita, posición y UV finitas, luz finita y material con valores radiométricos válidos. Los fallbacks de la tabla se prueban manteniendo esas entradas válidas; no se promete sanear geometría corrupta ni todos los valores extremos del material heredado.

## Pruebas (`tests/void_eye.rs`)

| Prueba | Qué fija | Origen |
|---|---|---|
| `void_eye_is_not_the_lit_planet_placeholder` | No es un alias de `LitPlanet` | RED del supervisor → GREEN |
| `the_pupil_faces_the_ship_left_or_right_wherever_the_sphere_is` | La pupila mira a la nave (< 10°) a izquierda, derecha y arriba-lejos, con la esfera fuera del origen | RED → GREEN |
| `the_single_corona_breathes_slowly_in_place_while_the_pupil_stays_on_the_ship` | La corona respira en su sitio (toda la banda cambia igual), con un solo ciclo lento de 3 s, sin cambios fuera del iris, acento rojo-ámbar y pupila idéntica en el tiempo. **Sustituye** en la 02 a `radial_rings_drift_outward_with_time_while_the_pupil_stays_on_the_ship` (contrato de anillos que se desplazan, conservado en `tests/fixtures/void_v1_tests.txt`) | RED → GREEN (02) |
| `adverse_ship_and_time_fall_back_stably_with_finite_results` | La tabla de entradas adversas | RED → GREEN |
| `void_eye_is_distinct_from_every_other_shader` | Distinto de Flat, LitPlanet, AnimatedSun y LucidSurface en la mayoría de los puntos | Guarda |
| `the_eye_ignores_camera_and_oxygen_and_depends_only_on_where_the_ship_is` | Idéntico bit a bit con otras cámaras y otro O₂ | Guarda; mutaciones (cámara como objetivo, O₂) |
| `moving_the_whole_scene_does_not_move_the_eye_on_the_sphere` | Trasladar escena, nave, luz y cámara deja el mismo color | Guarda; mutación (supuesto de origen) |
| `the_result_is_pure_deterministic_and_seamless_with_the_pupil_on_the_seam_or_pole` | Mismo resultado en cualquier orden de evaluación; sin costura con la pupila sobre u = 0; sin dispersión extra en el polo | Guarda |
| `a_real_render_keeps_geometry_and_the_pupil_moves_with_the_ship_on_screen` | `draw_model` real con las mismas `RenderStats` y silueta que `LitPlanet`; la pupila en pantalla va a la izquierda o a la derecha con la nave | Guarda |

**Variante 02 (`tests/void_eye_variant.rs`)**, comparada con la 01 de referencia:

| Prueba | Qué fija | Origen |
|---|---|---|
| `iris_is_one_continuous_broad_corona_not_separate_thin_rings` | Emisión continua en la meseta del iris, sin huecos entre anillos | RED del supervisor → GREEN |
| `the_pupil_its_soft_edge_and_the_surface_outside_the_iris_are_exactly_01` | Pupila, su borde suave, superficie fuera del iris y ojo cerrado idénticos bit a bit a la 01, con varias naves, tiempos, cámaras y O₂ | Guarda |
| `the_corona_keeps_the_01_band_envelope_palette_and_glow_ceiling` | La corona sigue exactamente la envolvente del iris de la 01, con color del segmento rojo → ámbar y brillo ≤ 0.12 | Guarda |
| `the_iris_band_is_one_connected_corona_without_inner_valleys_at_every_phase` | En todas las fases: una sola banda conectada, sin valles, llena en la meseta; la 01 da varios máximos en la misma banda | Guarda |

## Vista técnica

```sh
cargo run --release --locked --offline --example void_eye_preview -- --output-dir <carpeta-nueva>
```

La carpeta **debe ser nueva**. El ejemplo escribe **9 PNG** de 1280×720 y `manifest.json`.

**Configuración fija:** `uv_sphere(32, 64)` de radio 1 en el origen, material `(0.22, 0.23, 0.26)`, luz `(5, 2, 1)` y FOV 60°. La nave no se dibuja: solo su posición entra al shader.

| Tomas | Cámara | Nave |
|---|---|---|
| `base_lit`, `comparar_animated_sun`, `comparar_lucid_o2_005`, `void_eye` a t = 1.3 s | cerca `(0, 0.35, 2.6)` | frente `(0, 0.25, 1.9)` |
| `void_eye` a t = 1.3 s | cerca | izquierda `(−1.6, 0.2, 1.2)` y derecha `(1.6, 0.2, 1.2)` |
| `void_eye` a t = 2.8 s (medio periodo de la respiración) | cerca | frente |
| `base_lit`, `void_eye` a t = 1.3 s | juego `(0, 1.4, 10)` | frente `(0, 1.1, 7.5)` |

Las comparaciones del manifest (`checks`) se calculan sobre los píxeles **antes de etiquetar**:
- píxeles distintos frente a Lit, AnimatedSun y Lucid;
- entre las dos fases temporales;
- entre nave izquierda y derecha;
- centro horizontal de la pupila, definido como los píxeles con menos de 1/4 de la luminancia de la base.

## Integración en el juego (`--void-eye`)

**Qué cambia:** solo `ShaderKind` del planeta **Vigilia** existente (`LitPlanet` → `VoidEye`), una vez tras `World::new` y antes del primer cuadro. El shader es exactamente el de la variante 02 aprobada (sin retoques). Vigilia conserva su material real `(0.30, 0.38, 0.55)` (no el gris del preview), radio 20, órbita 520, velocidades, fase, LOD y descarte. Los demás cuerpos, la criatura, el Sol, la cámara y la nave no cambian.

**Rutas:**

| Ruta | Línea de comandos | Biblioteca |
|---|---|---|
| Juego normal (con o sin criatura estática y su revelación) | `--void-eye [--lucid-surface]` | `run_game_with_void_eye(options, lucid, false)` |
| Sol con ojo | `--creature-solar-reveal --void-eye [--lucid-surface]` | `run_game_with_void_eye(options, lucid, true)` |

- Se rechaza con `--legacy` y, fuera del Sol con ojo, con `--creature-eye-compare`, `--creature-distant-turn` o `--creature-horizontal-follow` (`validate_void_eye_flags`), antes de expandir los flags del Sol y antes de leer archivos.
- La entrada de biblioteca valida antes de cualquier IO lo mismo que la CLI: tamaño (`checked_mul` y máximo de píxeles), tiempo, O₂, modos de la criatura, arranque y alias de `--output`, más los guards de su ruta.
- Las entradas anteriores (`run_game`, `run_game_with_lucid_surface`, `run_game_with_solar_reveal`, etc.) y `GameOptions` no cambian.

**Selección de Vigilia** (`visual::void_game::void_body_index`): por nombre, en cada llamada (sin índice fijo ni guardado). Error sin mutar nada si falta, si está duplicada, si su shader no es `LitPlanet`/`VoidEye`, o si su radio, su órbita (finitos y > 0) o su velocidad orbital, rotación o fase (finitas) no valen. `set_void_eye(world, enabled)` es idempotente y `false` vuelve a `LitPlanet`.

**Reloj y objetivo:** el contexto del mundo ya trae `ship_position` y `elapsed_time` (tiempo de simulación). El ojo mira siempre a la **nave** (no a la cámara; `G` del Sol con ojo solo cambia la mirada de la criatura). `F5` congela el reloj y con él la respiración; si la nave se mueve, el ojo la sigue. El O₂ (`1`–`4`, `M`/`N`) no afecta al ojo. Sin teclas, HUD ni gameplay nuevos.

**Visibilidad:** el ojo solo cambia el cuadro cuando Vigilia está en él. Por ejemplo, en la vista FRONTAL del Sol con ojo a t = 10 s Vigilia queda fuera y el cuadro es idéntico al de sin flag (correcto); a t = 0 s sí se ve.

**Pruebas:** `tests/void_game.rs` (tracer del supervisor, identidad y atomicidad, combinaciones de la CLI, guards de la entrada de biblioteca en las cuatro combinaciones, ayuda, reversibilidad y material real, cuadros reales de la CLI en ambas rutas, bucle real con la nave, `F5`, cámara, `G` y rutas solares con una criatura sintética desechable).
