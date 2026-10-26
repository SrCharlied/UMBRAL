# Arquitectura de UMBRAL

Estado: **Hito 1 (Persona 1)** implementado en código; los contratos de la sección 5 existen tal como se describen abajo, pero **siguen pendientes de firma** (§5.2, UMB-003) y UMB-004 (§8) espera respuesta del profesor. Persona 2 y Persona 3 deben revisarlos antes de construir encima.

## 0. Lo implementado en el Hito 1

| Tarea | Dónde | Notas |
|---|---|---|
| UMB-101 | `renderer/pipeline.rs`, `scene/` | `draw_model` no limpia ni autoencuadra; dos instancias con `Transform` propio |
| UMB-102 | `math/`, `scene/camera.rs` | MVP, perspectiva con NDC `[-1,1]`, división por `w`, viewport con Y invertida |
| UMB-103 | `assets/obj.rs` | `vt`, `vn`, `v/vt/vn`, validación de rangos, esquinas mixtas rechazadas |
| UMB-104 | `renderer/rasterizer.rs::triangle_shaded` | Posición, normal y UV con corrección de perspectiva (`1/w`) |
| UMB-105 | `renderer/shader.rs` | Despacho por `ShaderKind`, solo para fragmentos que pasan el z-test |
| UMB-106 | `scene/ship.rs`, `scene/world.rs` | Empuje y giros suavizados con `dt`; cámara de seguimiento |
| UMB-107 | `scene/solar_system.rs` | Sol + 3 planetas; órbita y rotación son función pura del tiempo |
| UMB-402 (parte de Persona 1) | `scene/world.rs` | Tres esferas (32x64, 16x32, 8x16) elegidas por radio/distancia; `F4` y `--no-lod` para comparar |
| UMB-403 (base) | `debug/overlay.rs` | Fuente 5x7 propia y contadores; Persona 3 puede ampliarlo (shader activo, semilla) |
| Extra | `renderer/pipeline.rs` | Descarte de caras traseras por material (`cull_backfaces`), contador `triangles_backface` |
| UMB-401 (parcial) | `scene/world.rs`, `renderer/clipping.rs` | Descarte de objetos por esfera/frustum con interruptor `F3` y contadores |

Recorte: solo contra el plano cercano (es el que puede producir `w <= 0`). Los demás planos se resuelven con la caja de pantalla del rasterizador y el descarte trivial por códigos de salida.

## 1. Módulos existentes

| Módulo | Contenido | Propietario | Estado |
|---|---|---|---|
| `src/renderer/framebuffer.rs` | Color `u32` 0xRRGGBB (sRGB) y profundidad `f64`; rechaza fuera de pantalla y profundidad no finita | Persona 1 | Base reutilizable |
| `src/renderer/rasterizer.rs` | `triangle`: caja limitada a pantalla, baricéntricas en el centro del píxel, ambos órdenes, profundidad interpolada linealmente | Persona 1 | Base reutilizable |
| `src/renderer/image.rs` | `save_png`: framebuffer a PNG RGB de 8 bits | Persona 1 | Base reutilizable |
| `src/renderer/legacy.rs` | `render_model`: rotación Y, vista ortográfica fija, autoencuadre, Lambert plano por cara | Persona 1 | **Solo baseline**; se reemplaza en UMB-101/102 |
| `src/assets/obj.rs` | `parse_obj`/`load_model`: `v`, `f` triangulada (primer índice), `mtllib`, `usemtl`, `newmtl`, `Kd` | Persona 1 | Lector limitado; UMB-103 lo extiende |
| `src/assets/mod.rs` | `ship_model_path()` | Persona 1 | Ver decisión D5 |
| `assets/models/` | Nave OBJ/MTL | Persona 3 | Compartido con revisión |
| `src/config.rs` | Resolución por defecto (`DEFAULT_WIDTH`, `DEFAULT_HEIGHT`) | Compartido; cada persona añade sus parámetros | Recibirá los `*Settings` del plan §12 cuando existan |
| `src/main.rs` | CLI (`--width/--height/--frames/--output/--headless`), ventana minifb | Compartido | Se divide cuando exista bucle de juego (UMB-106) |
| `tests/` | 14 pruebas de integración | Compartido | Cada módulo mantiene las suyas |

Todo el código usa `f64` y arreglos `[f64; 3]`; no hay tipos `Vec3`.

## 2. Puntos de entrada planificados (no creados)

No se crearon carpetas vacías: cada una nace con la primera tarea que necesita código real.

| Carpeta | Propietario | La crea | Primer contenido |
|---|---|---|---|
| `src/math/` | Persona 1 | UMB-101/102 | `Transform`, vectores, matrices (según D2) |
| `src/scene/` | Persona 1 | UMB-101/106 | Objetos con transformación, cámara |
| `src/gameplay/` | Persona 2 | UMB-201/205 | `GameplayState`, oxígeno |
| `src/visual/` | Persona 3 | UMB-105/306 | Shaders concretos, Asfixia Lúcida |
| `src/debug/` | Persona 3 | UMB-310/403 | Oxígeno forzado, overlay |
| `src/renderer/shader.rs` | Persona 1 (Persona 3 con acuerdo) | UMB-105 | `ShaderKind`, `shade_fragment` |

## 3. Flujo actual

```text
OBJ + MTL ──load_model──► Model { vertices: [f64;3], faces, materials: Kd }
                               │
                legacy::render_model(fb, model, angle)
                   rota en Y → proyecta ortográfico → autoencuadra → clear()
                   por cara: normal plana, Lambert + ambiente, sRGB
                               │
                    rasterizer::triangle → Framebuffer::point (z-test)
                               │
              minifb (presenta el buffer)  |  image::save_png (headless)
```

## 4. Espacios de coordenadas

**Hoy (visor heredado):**

- **Objeto:** coordenadas del OBJ tal cual. Probablemente Y arriba (valor por defecto del exportador de Blender); no verificado. Escala en unidades de Blender, sin equivalencia de mundo.
- **Vista:** base ortonormal fija con `forward = normalizar(1.25, 1.15, 1.0)`; profundidad `z = -dot(v, forward)`; **menor = más cerca**.
- **Pantalla:** píxeles, x a la derecha, y hacia abajo; el muestreo usa el centro del píxel (+0.5).
- **Color:** `Kd` del MTL se trata como lineal y se convierte a sRGB por canal.

**Propuesta para el pipeline (UMB-102), pendiente de UMB-003:**
objeto → mundo (`Transform`) → vista (cámara) → recorte (proyección, `w`) → NDC (división por `w`) → viewport (píxeles). Falta acordar: mano del sistema, eje frontal de la cámara, rango de profundidad en NDC, orden de rotaciones de Euler. Ángulos en radianes en todo el código, como ya ocurre.

## 4.1 Resolución: render y presentación

**Decisión aprobada por Charlie:** resolución inicial del juego 1280×720 (16:9), configurable por `--width/--height`.

| Concepto | Qué es | Hoy |
|---|---|---|
| Resolución de render | Dimensiones del `Framebuffer`: píxeles que el rasterizador evalúa y que exporta el PNG | `config::DEFAULT_*` o la CLI |
| Resolución de presentación | Tamaño de la ventana donde minifb muestra el buffer | **Igual** a la de render: `Window::new(width, height, WindowOptions::default())`, que en minifb 0.28 implica `resize: false` y `Scale::X1` |

- **Fuente única de valores:** `src/config.rs`; la ayuda de la CLI y la validación los leen de ahí y de `framebuffer::MAX_PIXELS`.
- **Validación:** `Framebuffer::new` rechaza producto cero, desbordamiento (`checked_mul`) y más de `MAX_PIXELS` (16 777 216) **antes** de reservar memoria. La CLI solo rechaza valores faltantes, no enteros o cero.
- **Fuera de este incremento:** escalado entre render y presentación, ventana redimensionable, relación de aspecto de cámara (llegará con la proyección de UMB-102), resolución interna reducida como recorte de rendimiento (plan §14).
- **Baseline heredado:** se mantiene a 1280×960 y se reproduce con `--width 1280 --height 960`; el visor heredado autoencuadra con cualquier aspecto, así que el render a 720p es otra imagen, no una regresión.
- **Rendimiento:** no medido. No existe todavía una escena del juego con la que tenga sentido medir.

## 5. Contratos compartidos (UMB-003)

Estado: **implementados, pendientes de firma** (ver §5.2). Fuente de verdad: el código; esta tabla resume rangos, unidades y espacios. Todo es `f64` (D1), ángulos en radianes, distancias en unidades de mundo, tiempo en segundos.

| Contrato | Dónde | Produce | Consume | Rangos y espacios |
|---|---|---|---|---|
| `Transform { position, rotation, scale }` | `math/matrix.rs` | Escena | Renderer | Posición en mundo; rotación Euler en radianes con `R = Ry·Rx·Rz`; `M = T·R·S`; escala finita y > 0 |
| `RasterVertex { x, y, z, inverse_w, world_position, normal, uv }` | `renderer/rasterizer.rs` | Etapa de vértices (Persona 1) | Rasterizador | Sustituye al `VertexOut` del plan. `x, y` en píxeles; `z` profundidad NDC; `inverse_w = 1/w` para interpolar con corrección de perspectiva. `clip_position` no se conserva tras el recorte |
| `FragmentInput { screen_position, world_position, normal, uv }` | `renderer/shader.rs` | Rasterizador (Persona 1) | Shaders (Persona 3) | `screen_position` en píxeles + profundidad; `normal` reinterpolada y renormalizada; solo se crea si el fragmento pasa el z-test |
| `ShaderContext { elapsed_time, oxygen01, camera_position, ship_position, light_position }` | `renderer/shader.rs` | Persona 1 transporta; Persona 2 produce `oxygen01` | Persona 3 | `elapsed_time` ≥ 0 s; `oxygen01` en `0.0..=1.0` (lo acota quien lo produce); posiciones en mundo |
| `ShaderKind` + `shade_fragment(kind, fragment, context, material)` | `renderer/shader.rs` | Persona 1 (punto de extensión) | Persona 3 (implementaciones) | Una llamada por fragmento que pasa el z-test. `LucidSurface` y `VoidEye` hoy reutilizan `LitPlanet` |
| `Material { base_color, use_model_colors, ambient, cull_backfaces }` | `renderer/shader.rs` | Escena / Persona 3 | Shaders, pipeline | Colores lineales en `0..=1`, `ambient` en `0..=1`. Resuelve D4 en su forma mínima; Persona 3 puede ampliarlo con aviso |
| `draw_model(...) -> RenderStats` | `renderer/pipeline.rs` | Persona 1 | Escena, debug | No limpia el framebuffer ni autoencuadra |
| `RenderStats` | `renderer/stats.rs` | Renderer | Overlay | Contadores de objetos, vértices, triángulos (enviados, descartados, recortados, caras traseras, rasterizados), fragmentos y píxeles escritos |
| `Camera` | `scene/camera.rs` | Persona 1 | Persona 2 (API pública) | Mano derecha, Y arriba, mira a −Z en vista; NDC `[-1,1]` |
| `GameplayState { oxygen01, collected_resources, collected_logs, game_over, objective_complete }` | `gameplay/` (**no existe aún**) | Persona 2 | Persona 3 (solo `oxygen01`) | `oxygen01` en `0.0..=1.0`; contadores monotónicos hasta reinicio. Hoy `World` usa un valor de depuración: `TODO(UMB-205)` |

### 5.1 Qué produce y qué consume cada persona

- **Persona 1:** `Transform`, matrices, `Camera`, `draw_model`, `FragmentInput`, `RenderStats`. Transporta `ShaderContext` sin interpretarlo.
- **Persona 2:** `GameplayState`; entrega `oxygen01` y la posición de la nave. Consume `Camera` y `Transform` por su API pública.
- **Persona 3:** implementa `shade_fragment` para cada `ShaderKind`, `Material` y contenido de escena. Consume `FragmentInput`, `ShaderContext` y `oxygen01`.

### 5.2 Registro de aceptación

Un contrato queda cerrado cuando cada persona afectada lo confirma. Completar la fecha al aceptar; cambios posteriores solo por conversación entre los afectados.

| Persona | Contratos que debe confirmar | Aceptado (fecha) |
|---|---|---|
| Persona 1 | Todos (autora del pipeline) | |
| Persona 2 | `GameplayState`, `ShaderContext.oxygen01`, `Camera`, `Transform` | |
| Persona 3 | `ShaderKind`, `shade_fragment`, `FragmentInput`, `ShaderContext`, `Material` | |

## 6. Decisiones pendientes

| ID | Decisión | Quién decide | Notas |
|---|---|---|---|
| D1 | `f32` o `f64` para el pipeline nuevo | Persona 1 + equipo | **Resuelta provisionalmente: `f64`** en todo el código nuevo (el plan escribe `f32`). Cambiarlo es mecánico pero toca todos los módulos |
| D2 | ¿Se permite una biblioteca matemática? ¿Se conserva `minifb` o se espera Raylib? | Equipo con el profesor (UMB-004) | **Sin respuesta del profesor.** Ver §8. Mientras tanto: `minifb` y `png` fijados, matemáticas propias, ninguna biblioteca 3D |
| D3 | Convenciones de la sección 4 (mano, eje frontal, NDC, orden de Euler) | Persona 1 (UMB-003) | **Implementada, pendiente de firma en §5.2:** mano derecha, Y arriba, la cámara mira a −Z, NDC `[-1,1]`, `R = Ry·Rx·Rz`, `M = T·R·S`. Falta que el equipo la acepte |
| D4 | Estructura de `Material` | Persona 1 + Persona 3 | **Resuelta provisionalmente** con `base_color`, `use_model_colors`, `ambient`, `cull_backfaces`; se confirma en §5.2 |
| D5 | Resolución de assets: `CARGO_MANIFEST_DIR` o relativa al ejecutable | Persona 1 | Hoy el binario no es reubicable |
| D6 | Baseline exacto en otras plataformas | Equipo | Solo verificado en Windows x86_64 con Rust 1.97.0. Si falla en otro sistema, investigar el redondeo antes de regenerar el fixture o introducir tolerancia |
| D7 | Ruta por defecto de `P` (`renders/nave.png`, no ignorada) | Persona 3 | Comportamiento heredado sin cambios |
| D8 | Modelo de prueba con UV para UMB-103/104 | Persona 1 + Persona 3 | La nave no tiene `vt` |
| D9 | Asignación de Persona 1, 2 y 3 | Equipo | — |
| D11 | Hacia dónde apunta el OBJ de la nave | Persona 3 | `SHIP_MODEL_YAW = π` en `scene/world.rs` alinea el modelo con −Z; se verificó solo visualmente |
| D10 | Escalado render → presentación y ventana redimensionable | Persona 1 + Persona 3 | Hoy no existe; decidir si la resolución de render puede ser menor que la ventana (plan §14) |

## 7. Validación

Los cuatro comandos del README (formato, Clippy, pruebas, render headless) son el gate de cada rama. `tests/ship_baseline.rs` protege el visor heredado: mientras `legacy.rs` exista, el fixture `tests/fixtures/legacy_ship_1280x960.png` no se regenera para hacer pasar una prueba. Cuando UMB-101/102 sustituyan el visor, el equipo decide si se retira la prueba o se crea un baseline nuevo con su propio origen documentado.

## 8. Restricciones de bibliotecas (UMB-004)

**Estado: pendiente de respuesta del profesor.** Postura del equipo mientras tanto, que es válida con cualquier respuesta salvo "se exige Raylib":

| Pieza | Decisión actual | Lo que hace | Lo que no hace |
|---|---|---|---|
| `minifb 0.28.0` | Ventana, teclado y presentar el buffer | Muestra un arreglo de píxeles | No dibuja, no carga geometría, no tiene 3D ni GPU |
| `png 0.18.1` | Exportar capturas headless | Codifica PNG | Nada de render |
| Matemáticas | Propias (`math/`: `Vec2/3/4`, `Mat4`, `Transform`) | Todo el MVP | Sin biblioteca externa; si se autoriza una, el cambio es mecánico |
| Rasterizado, z-buffer, shaders, OBJ | Propios | Todo el pipeline | — |

Regla independiente de la respuesta: no usar funciones 3D, shaders de GPU ni física externa.

Si el profesor exige Raylib: se sustituye `minifb` en `src/app.rs`, que es el único módulo que toca la ventana; el renderer entrega un buffer de píxeles y no cambia.

Registro de la consulta (completar):

| Fecha | A quién | Pregunta | Respuesta |
|---|---|---|---|
| | | ¿Puede conservarse `minifb` (solo presenta un buffer) en lugar de Raylib? ¿Se permite una biblioteca matemática o deben ser propias? | |
