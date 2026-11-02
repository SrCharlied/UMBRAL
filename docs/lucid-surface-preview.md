# Superficie lúcida (UMB-308)

**Estado:** la variante 02 está **aprobada visualmente por Charlie** e integrada en el juego **solo de forma opt-in** con `--lucid-surface` sobre el planeta Antena existente (ver [Integración en el juego](#integración-en-el-juego---lucid-surface)). Sin el flag no cambia ningún planeta: `SolarSystem::demo` sigue usando `LitPlanet`. No es la identidad final de Antena. Código en `src/renderer/shader.rs` (`ShaderKind::LucidSurface`), pruebas en `tests/lucid_surface.rs` y vista técnica sin ventana en `examples/lucid_surface_preview.rs`. Sin modelos ni texturas externas.

## Qué hace

Es una piedra oscura con **venas y grietas rojo-ámbar discretas** que aparecen poco a poco al bajar el oxígeno y respiran con una pulsación lenta y limitada.

| Oxígeno (`ShaderContext::oxygen01`) | Resultado |
|---|---|
| `≥ 0.50`, o no finito (`NaN`, `±Inf`) | **Exactamente** `LitPlanet`, bit a bit. |
| `0.50 → 0.05` | Intensidad `1 − smoothstep(0.05, 0.50, O₂)`: continua, de derivada nula en ambos extremos y sin saltos en 50 % ni en 25 %. |
| `≤ 0.05` | Intensidad máxima (saturada). |

- **No depende de un historial.** El color es función pura del fragmento, del contexto y del material, así que bajar y volver a subir el O₂ da exactamente lo mismo en cada nivel.
- **Rango del O₂.** Un valor finito fuera de `0..=1` se recorta y uno no finito se trata como estable.
- **Tiempo.** Un `elapsed_time` no finito cuenta como `t = 0`, pero **solo en este shader**. `AnimatedSun` y los demás no cambian.
- **Gameplay.** Nada de consumo de O₂ ni de recursos: el preview fija el oxígeno a mano en el contexto.

### Cómo se construye

1. **Base.** Es `lit_planet` sin modificar: el difuso, el terminador y el patrón no emisivo se conservan. Entre grietas el color es **idéntico** a `LitPlanet`, incluso a O₂ 5 %.
2. **Dominio.** Es la dirección de la esfera unidad derivada de la UV, con la convención de `uv_sphere`: `(sin πv·cos 2πu, cos πv, sin πv·sin 2πu)`.
   - Es periódica en `u`, así que no añade costura en u = 0/1.
   - Es continua en los polos.
   - Está anclada al planeta: **no** usa `world_position`, así que el patrón viaja con la rotación.
3. **Grietas.** Voronoi sobre la esfera:
   - 48 centros en espiral de Fibonacci con desorden determinista (SplitMix64).
   - Se calcula la distancia exacta al borde de la placa, es decir, el mínimo a los planos bisectores, que pasan todos por el centro.
   - Por eso el ancho es uniforme y no hay rejilla 3D cortando la esfera.
   - Una leve ondulación del dominio quita el aspecto geométrico.
4. **Aparición por zonas.** Un campo suave (suma de senos de la dirección) asigna a cada zona un umbral de encendido. Las primeras zonas se encienden en cuanto el O₂ baja de 50 % y desde intensidad 0.8 están todas.
5. **Venas, no malla.** Otro campo suave deja **dormida** parte de la red, que queda como fisura oscura y fina; otro más varía el brillo a lo largo de cada vena.
6. **Color.**
   - Borde rojo profundo `(0.55, 0.06, 0.02)` hacia un núcleo ámbar `(1.0, 0.42, 0.08)`, en lineal.
   - La emisión se **suma** a la base, así que las venas se ven también en el lado nocturno.
   - Un borde carbonizado estrecho oscurece la piedra junto a la grieta.
7. **Pulsación.** Coseno de periodo **4.5 s** que solo modula el brillo de la emisión, entre 75 % y 100 %. Nunca mueve el patrón ni lo apaga. A 60 fps el mayor cambio por cuadro medido es `0.003` en lineal.

Las constantes (umbrales 50 %/5 %, paleta, 48 placas, ancho 0.02 radios y periodo 4.5 s) forman parte de la apariencia aprobada con la variante 02; la integración no los reajusta.

## Variante 02: tramo crítico sin «balón» (aprobada por Charlie)

**Motivo:** feedback de Charlie sobre la 01 a O₂ 5 %: «en algunas sí se ve como un balón». La red completa de placas, con sus uniones en Y y la fisura carbonizada que también las delimita, se lee como paneles cerrados. La variante 02 solo actúa en el tramo final y conserva todo lo demás.

| Oxígeno | Resultado |
|---|---|
| `≥ 0.15` (y por tanto todo `≥ 0.25`) | **Bit a bit** la 01. |
| `0.15 → 0.05` | Entra con `critical = 1 − smoothstep(0.05, 0.15, O₂)`, continua y sin saltos en 15 % ni en 5 %. |
| `≤ 0.05` | Completa (saturada). |

**Mecanismo** (todo en el dominio de las placas, es decir, la dirección derivada de la UV, determinista y sin depender del tiempo, la cámara ni `world_position`):
1. **Borrar aristas enteras.** Se calcula una vez la lista de aristas de Voronoi (pares de placas entre dos uniones triples).
   - Se seleccionan las aristas cuyo hash del par de placas es menor que 0.36. Es un umbral determinista, no una cuota exacta del 36 %.
   - También se borran siempre las aristas muy cortas (< 0.06 rad). Si no, quedarían como un punto suelto cuando desaparecen sus vecinas.
   - Cada arista seleccionada pierde tanto la grieta brillante como la fisura carbonizada. No se garantiza que todas las placas pierdan al menos un lado.
2. **Desviar en vez de cerrar.** La banda que borra cada arista se desvanece al llegar a la unión.
   - Con una sola arista borrada en una unión, los otros dos brazos siguen unidos por el vértice: la grieta gira en V o en L, sin Y.
   - Con dos o tres aristas borradas, las bandas suman y el vértice también desaparece.
3. **Continua por construcción.** El borrado es una suma de bandas suaves sobre aristas fijas, así que no depende de qué arista está más cerca. Una versión anterior que sí dependía de eso dejaba saltos de color de hasta 0.70 en vértices casi degenerados; lo detectó una prueba.
4. **No es un mero oscurecimiento.** En el tramo crítico:
   - Las venas dormidas reciben hasta un 85 % de brillo, con una mezcla lineal de pendiente acotada.
   - Las grietas que quedan se ensanchan hasta un 50 % según un campo suave, lo que da un ancho variable a lo largo de la grieta.
   - Resultado: el acento total queda en torno al 95 % del de la 01 y crece al bajar el O₂.

El tiempo sigue modulando solo el brillo (75–100 %): nunca mueve ni reabre lo borrado. La paleta, los umbrales 50/5, el pulso, las 48 placas, la semilla, la luz base y la forma no cambian. Las constantes `EDGE_ERASED_SHARE`, `ERASE_*`, `CRITICAL_WAKE` y `CRITICAL_WIDEN` quedan como se aprobaron; la integración no los toca.

## Precondiciones de entrada

Este shader **no** sanea todo el pipeline. Supone fragmentos normales de `draw_model`:
- `normal` y `world_position` finitas;
- UV de esfera equirectangular en `0..=1`, como las de `uv_sphere`;
- `Material` con color y `ambient` finitos.

Con UVs de otra parametrización, como un OBJ arbitrario, el patrón sigue siendo continuo, pero deja de corresponder a la superficie real. Solo se sanean `oxygen01` y `elapsed_time` no finitos. No se promete un recorte universal: con entradas geométricas corruptas, el resultado es el que dé `lit_planet`.

## Pruebas (`tests/lucid_surface.rs`)

Los incrementos funcionales siguieron ciclos RED → GREEN; las guardas que nacieron en verde se distinguen abajo y, para anclaje y polos, se verificaron también mediante mutaciones temporales. Los logs están en la evidencia del trabajador.

| Prueba | Qué fija |
|---|---|
| `critical_lucid_surface_is_not_the_lit_planet_placeholder` | Primer tracer bullet del supervisor (sin cambios). |
| `low_oxygen_draws_localized_red_amber_cracks_not_a_global_tint` | A O₂ 5 %: ≥ 40 % intacto, ≤ 35 % con acento, ≥ 2 % de acento fuerte, tono rojo-ámbar. |
| `oxygen_at_or_above_half_is_exactly_the_lit_planet` | Colores bit a bit y render real (píxeles y `RenderStats`) iguales a `LitPlanet` con O₂ 100/90/75/60/50 %. |
| `oxygen_response_is_continuous_monotonic_saturated_and_stateless` | Sin saltos (< 0.02 entre pasos de 0.1 % de O₂), crece al bajar el O₂, se satura en 5 % y no tiene historial. |
| `cracks_add_no_seam_at_u_wrap_and_no_singularity_at_the_poles` | u = 0 ≡ u = 1 y continuidad a 1e-9; en los polos no añade dispersión sobre la de `LitPlanet` (+0.01). |
| `cracks_are_anchored_to_the_planet_uv_not_to_world_space` | Al girar el planeta con la luz y la cámara, el color no cambia (< 1e-9): el patrón no depende de `world_position`. |
| `time_only_breathes_the_cracks_slowly_without_moving_or_blacking_out` | En 20 s a 60 fps: mismo conjunto de puntos tocados, mínimo ≥ 65 % del máximo, periodo de 3 a 10 s y < 0.01 por cuadro. |
| `invalid_oxygen_is_stable_and_invalid_time_is_sanitized_without_new_nans` | O₂ `NaN`/`±Inf` dan exactamente `LitPlanet`; O₂ < 0 equivale a 0; tiempo no finito equivale a 0; nunca NaN ni Inf nuevos. |
| `at_game_distance_the_cracks_are_visible_and_the_silhouette_is_unchanged` | A 1280×720: misma silueta y `RenderStats`; ≥ 4 % del disco con acento visible, ≤ 45 % cambiado y ≤ 25 % rojizo. |

Las guardas de anclaje y de polos nacieron en verde. Se comprobó con mutaciones temporales que detectan su defecto: el patrón por `world_position` y una proyección cilíndrica, respectivamente.

**Caveat honesto:** con esta semilla no pasa ninguna grieta exactamente por los polos. La prueba de polos protege sobre todo contra proyecciones que dependen de `u`.

### Variante 02 (`tests/lucid_surface_variant.rs`)

El primer test es del supervisor y se conserva sin cambios. La referencia exacta de la 01 se compila desde `tests/fixtures/lucid_v1_shader.rs` (módulo `reference`) para comparar con ella en cada punto. Las 9 pruebas anteriores de la 01 siguen pasando con la variante, sin cambiar ni su cuerpo ni sus tolerancias.

| Prueba | Qué fija | Origen |
|---|---|---|
| `five_percent_variant_changes_the_original_closed_plate_pattern` | A 5 % la red cambia respecto de la 01 (> 1 % de los puntos). No certifica nada artístico. | RED del supervisor → GREEN |
| `critical_variant_breaks_most_triple_junctions_asymmetrically` | Un oráculo independiente localiza las uniones triples de la 01. Al menos 2/3 de las que estaban cerradas se rompen, y ≤ 1/3 de las rotas son «agujeros redondos» (centro apagado con los tres brazos presentes). | RED → GREEN |
| `critical_variant_opens_most_closed_plates_including_the_charred_network` | Placas cerradas (componentes conexas de piedra, contando también la fisura oscura): la mitad o menos que en la 01. | Guarda, nació en verde; verificada por mutación |
| `critical_variant_is_not_mere_dimming_and_keeps_legible_accents` | Acento total ≥ 90 % y acento fuerte ≥ 85 % de la 01, a tres tiempos del pulso. | RED → GREEN |
| `breaking_the_network_adds_no_color_jumps_around_the_junctions` | En círculos finos alrededor de las 92 uniones no hay saltos mayores que la pendiente legítima de un borde. | RED (salto de 0.70) → GREEN |
| `from_fifteen_percent_up_the_candidate_is_bit_exact_with_the_reference_01` | Igual bit a bit a la 01 desde 15 % hacia arriba, con 3 luces, 6 tiempos (incluido `NaN`), normales perturbadas y 2 materiales. | Guarda; mutación (inicio del tramo en 30 %) |
| `the_critical_stretch_is_continuous_saturated_and_differs_from_01_only_below_15_percent` | Barrido de 25 % a 0 % con saltos < 0.01 por paso de 0.05 %; saturado a 5 % o menos; cambia desde 14.9 %. | Guarda |
| `time_only_breathes_the_variant_and_never_moves_or_reopens_what_it_erased` | Con O₂ 12 % y 5 % y tiempos hasta `±f64::MAX`: mismo conjunto de puntos tocados, brillo entre 70 y 100 % y siempre finito. | Guarda; mutación (borrado dependiente del tiempo) |
| `critical_variant_keeps_seam_poles_anchoring_and_invalid_input_contract` | Contrato de la 01 a O₂ 12/10/7 %: costura, polos, anclaje a la UV, O₂ no finito y tiempo no finito. | Guarda; mutación (borrado anclado al mundo) |
| `native_renders_keep_geometry_and_the_critical_accent_stays_visible_at_game_distance` | `draw_model` real a 1280×720 (juego, cerca y giro de 35°): mismas `RenderStats` y silueta; a 5 %, ≥ 3 % del disco con acento visible. | Guarda |

No se promete robustez frente a fragmentos corruptos arbitrarios: rigen las mismas precondiciones de la 01 (material finito y UV de esfera).

## Integración en el juego (`--lucid-surface`)

Integración técnica de la variante 02 aprobada, **sin cambios en el shader** (`src/renderer/shader.rs` es el aceptado) ni en su apariencia.

**Qué cambia:** solo `CelestialBody::shader` de Antena pasa de `LitPlanet` a `LucidSurface`. Se activa una vez, después de `World::new`, con `visual::lucid_game::set_lucid_surface`. Así lo usan todas las rutas de dibujo: el cuadro inicial, headless, cada cuadro del bucle de ventana y `P`.

**Qué no cambia:**
- Antena conserva su material `(0.45, 0.40, 0.36)`, su radio 14, la órbita 220 con sus velocidades, la rotación, el LOD y el descarte.
- El Sol, Osario, Vigilia, la cámara, la nave, la luz y las teclas siguen igual.
- No cambian `ShaderContext` ni `GameOptions`. El shader recibe el O₂ y el tiempo vivos del mundo (`--debug-oxygen`, `1`–`4`, `M`/`N`, `--time`); `F5` conserva su significado.
- No hay consumo de O₂, estado de gameplay, HUD ni rótulos.

**Contrato del helper:**
- `lucid_body_index` busca Antena **por nombre** en cada llamada, sin guardar un índice.
- Exige un solo cuerpo con ese nombre, que no sea el Sol, con radio y órbita finitos y mayores que 0, y con shader `LitPlanet` o `LucidSurface`.
- Si algo no valida devuelve `Err` sin tocar el mundo.
- `set_lucid_surface(world, true/false)` es idempotente; desactivar vuelve a `LitPlanet` y deja el mundo idéntico al original.

| Ruta | Entrada | Notas |
|---|---|---|
| Juego normal | `--lucid-surface`, o `app::run_game_with_lucid_surface` | `--near 1` arranca junto a Antena (`place_near(1, 25)`) y `R` reinicia la nave como siempre. Admite la criatura estática y su revelación, como el juego normal. |
| Sol con ojo | `--creature-solar-reveal --lucid-surface`, o `app::run_game_with_solar_and_lucid_surface` | La composición, las teclas `7`/`8`/`R`/`G`/`H` y el título no cambian; Antena aparece lúcida cuando está en cuadro. |

**Combinaciones rechazadas antes de cualquier archivo:**
- `--legacy` con `--lucid-surface`.
- Fuera del Sol con ojo, `--creature-eye-compare`, `--creature-distant-turn` o `--creature-horizontal-follow`: se rechazan con un error claro en vez de apagar una ruta en silencio.
- Con el Sol con ojo, su propio guard sigue decidiendo: los flags que implica son redundantes y válidos, y `--creature-distant-turn` sigue prohibido.

`validate_lucid_surface_flags` se valida antes de expandir los flags implícitos del Sol. Las dos entradas de biblioteca validan, antes de cualquier IO, lo mismo que la línea de comandos:
- tamaño positivo y dentro de `MAX_PIXELS`;
- `time` finito y ≥ 0;
- O₂ en `[0, 1]`;
- modos de la criatura, `--near` frente a `--near-creature` y alias de `--output` (la ruta solar, con sus guards de siempre).

**Pruebas** (`tests/lucid_game.rs`; el primer test es del supervisor y se conserva sin cambios):

| Prueba | Qué fija | Origen |
|---|---|---|
| `lucid_flag_renders_antena_with_the_approved_shader_in_the_real_game` | PNG de la CLI (320×180, `--near 1`, t 1.3, O₂ 5 %, sin LOD) = `World::render` con solo `bodies[1].shader = LucidSurface`. | RED del supervisor → GREEN |
| `the_target_is_validated_before_any_change_and_errors_leave_the_world_intact` | Antena ausente, duplicada, Sol, radio u órbita no válidos, o shader ajeno: `Err` sin mutar. Si Antena cambia de posición en la lista, se elige por nombre. | RED → GREEN |
| `the_command_line_rejects_lucid_combinations_outside_its_two_routes_before_any_io` | `--legacy` y las rutas de comparación fuera del Sol: error claro, sin leer assets ni escribir PNG. | RED → GREEN |
| `the_solar_route_with_the_flag_draws_antena_lucid_in_the_approved_composition` | PNG de la CLI solar = cuadro de `SolarGame` con Antena lúcida (criatura sintética desechable). | RED → GREEN |
| `the_new_library_entries_have_the_command_line_guards_before_any_io` | Guards de tamaño, tiempo, O₂, modos, arranque y salida en las dos entradas nuevas. | RED → GREEN |
| `the_flag_validator_and_the_help_describe_the_two_routes` | Validador puro y `--help` (sin rutas privadas). | RED (ayuda) → GREEN |
| `only_antena_changes_shader_and_disabling_restores_the_original_world` | Solo cambia el shader de Antena, la operación es idempotente y desactivar restaura el mundo. | Guarda; mutaciones (desactivar no restaura, el helper toca el material) |
| `the_cli_frames_are_the_original_game_at_high_oxygen_and_antena_lucid_below_half` | O₂ 100/60/50 %: idéntico al juego sin flag. O₂ 25/5 %: el cuadro esperado, con mismas `RenderStats` y silueta; sin flag, el juego de siempre. | Guarda; mutación (activado siempre) |
| `in_the_real_loop_oxygen_steps_change_only_antenas_shader_frame_by_frame` | Bucle real (`advance_frame`, presets 1–4 y M/N, nave en movimiento): cada cuadro es el de referencia, idéntico al juego con O₂ ≥ 50 % y distinto por debajo. | Guarda; mutación (activar no hace nada) |
| `the_solar_composition_keeps_its_frame_at_high_oxygen_and_only_antena_changes_below_half` | Sol con ojo a O₂ 100/50/25/5 %: mismas estadísticas, silueta y título; mismos píxeles con O₂ ≥ 50 %. | Guarda |
| `the_solar_library_entry_and_redundant_flags_give_the_expected_frame` | Entrada de biblioteca solar y CLI con los flags redundantes: el cuadro esperado. | Guarda; mutación (la entrada solar olvida el flag) |
| `the_plain_route_also_accepts_the_static_creature_and_its_reveal` | La ruta normal con criatura estática y revelación funciona con el flag. | Guarda |

## Vista técnica

```sh
cargo run --release --locked --offline --example lucid_surface_preview -- --output-dir <carpeta-nueva>
```

La carpeta **debe ser nueva**: el ejemplo usa `create_dir` y falla si ya existe.

**Salida:** exactamente **14 PNG** de 1280×720 y `manifest.json`. Los nombres y los metadatos son deterministas, sin rutas absolutas ni marcas de tiempo.

**Configuración fija:**
- Geometría `uv_sphere(32, 64)` de radio 1, el LOD 0 del juego.
- Material `(0.22, 0.23, 0.26)`, ambient 0.04, con descarte de caras traseras.
- Luz en `(5, 2, 1)`, FOV 60°.
- Cámaras «cerca» `(0, 0.35, 2.6)` y «juego» `(0, 1.4, 10)`, esta última con un radio aparente de unos 62 px.

| Tomas | Cámara |
|---|---|
| `baseline_lit` + `lucid_o2_{100,050,040,025,005}` a t = 1.3 s | cerca |
| `lucid_o2_{025,005}` a t = 0 s (máximo del pulso) y t = 2.25 s (mínimo) | cerca |
| `lucid_o2_005` con el planeta girado +35° (el patrón viaja con la superficie) | cerca |
| `baseline_lit`, `lucid_o2_025` y `lucid_o2_005` | juego |

**Etiquetas y comparación exacta:**
- Las etiquetas van en una franja inferior, desde la fila 664. Antes de etiquetar, el ejemplo comprueba que la esfera no toca esa franja.
- Antes de etiquetar cuenta también los píxeles distintos frente a la toma `LitPlanet` de la misma cámara y giro 0, y los anota en el manifest.
- La comparación píxel a píxel con la base se puede rehacer sobre los PNG en la región de las filas `< 664`.

## Límites visuales conocidos

- Donde una vena se apaga cerca de un vértice de placa queda un trazo corto algo difuso.
- A distancia de juego las venas tienen 1–2 px de ancho. Se leen, pero son finas, y con una ventana más pequeña podrían perderse.
- No hay relieve: es puramente de shader y no cambia forma, silueta, profundidad ni normales. En macro, las grietas se ven pintadas, no hundidas.
- La paleta, la cantidad de placas y la pulsación sincronizada en todo el planeta son decisiones artísticas pendientes de Charlie.
- **Variante 02:**
  - La elección de qué aristas se borran es aleatoria (por hash), así que el resultado depende de la vista. En algunas tomas todavía queda alguna placa cerrada (en la toma frontal cercana, un cuadrilátero), y en otras el centro queda escaso.
  - En el vértice de una V la grieta se atenúa un poco: allí la banda de la arista borrada todavía resta un 35 %.
