# Arquitectura de UMBRAL

Estado: **Hito 0**. Este documento separa lo que existe en el código de lo que solo está propuesto. Nada de la sección «Contratos propuestos» está acordado ni implementado: UMB-003 y UMB-004 siguen abiertas.

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

## 5. Contratos propuestos (UMB-003, no acordados)

Tomados de `UMBRAL_PLAN_TECNICO.md` §5. El plan escribe `f32`; el código usa `f64` (ver D1).

| Contrato | Produce | Consume | Rangos y espacios propuestos |
|---|---|---|---|
| `Transform { position, rotation, scale }` | Persona 1 (matemática); escena | Renderer | posición en mundo; rotación Euler en radianes (orden pendiente); escala finita y > 0 |
| `VertexOut { clip_position, world_position, world_normal, uv, inverse_w }` | Vertex stage (Persona 1) | Rasterizador | `inverse_w = 1/w` para interpolación con corrección de perspectiva; normal unitaria en mundo |
| `FragmentInput { screen_position, world_position, normal, uv }` | Rasterizador (Persona 1) | Shaders (Persona 3) | `screen_position` en píxeles + profundidad; normal reinterpolada y renormalizada |
| `ShaderContext { elapsed_time, oxygen01, camera_position, ship_position, light_position }` | Persona 1 transporta; Persona 2 produce `oxygen01` | Persona 3 | `elapsed_time` en segundos ≥ 0; `oxygen01` en `0.0..=1.0`, acotado por quien lo produce; posiciones en mundo |
| `GameplayState { oxygen01, collected_resources, collected_logs, game_over, objective_complete }` | Persona 2 | Persona 3 (solo `oxygen01`) | `oxygen01` en `0.0..=1.0`; contadores monotónicos hasta reinicio |
| `ShaderKind` + `shade_fragment` | Persona 1 (punto de extensión) | Persona 3 (implementaciones) | Despacho por `enum`, llamado una vez por fragmento que pasa el z-test |
| `draw_model(...) -> RenderStats` | Persona 1 | Escena y debug | No limpia el framebuffer; reemplaza el autoencuadre |
| `RenderStats` | Renderer | Overlay (UMB-403) | Hoy existe en `legacy.rs` con `faces_processed` y `pixels_written`; se ampliará con triángulos descartados y fragmentos evaluados |
| `Material` | Assets | Shaders | **Sin definir.** Hoy es `[f64; 3]` (`Kd`) por nombre |

## 6. Decisiones pendientes

| ID | Decisión | Quién decide | Notas |
|---|---|---|---|
| D1 | `f32` o `f64` para el pipeline nuevo | Persona 1 + equipo | La importación conservó `f64` para no alterar el baseline. No mezclar el cambio con otra tarea |
| D2 | ¿Se permite una biblioteca matemática? ¿Se conserva `minifb` o se espera Raylib? | Equipo con el profesor (UMB-004) | Sin confirmar. No se agregó ninguna dependencia |
| D3 | Convenciones de la sección 4 (mano, eje frontal, NDC, orden de Euler) | Persona 1 (UMB-003) | Necesario antes de UMB-102 |
| D4 | Estructura de `Material` y si los shaders leen `Kd` | Persona 1 + Persona 3 | — |
| D5 | Resolución de assets: `CARGO_MANIFEST_DIR` o relativa al ejecutable | Persona 1 | Hoy el binario no es reubicable |
| D6 | Baseline exacto en otras plataformas | Equipo | Solo verificado en Windows x86_64 con Rust 1.97.0. Si falla en otro sistema, investigar el redondeo antes de regenerar el fixture o introducir tolerancia |
| D7 | Ruta por defecto de `P` (`renders/nave.png`, no ignorada) | Persona 3 | Comportamiento heredado sin cambios |
| D8 | Modelo de prueba con UV para UMB-103/104 | Persona 1 + Persona 3 | La nave no tiene `vt` |
| D9 | Asignación de Persona 1, 2 y 3 | Equipo | — |
| D10 | Escalado render → presentación y ventana redimensionable | Persona 1 + Persona 3 | Hoy no existe; decidir si la resolución de render puede ser menor que la ventana (plan §14) |

## 7. Validación

Los cuatro comandos del README (formato, Clippy, pruebas, render headless) son el gate de cada rama. `tests/ship_baseline.rs` protege el visor heredado: mientras `legacy.rs` exista, el fixture `tests/fixtures/legacy_ship_1280x960.png` no se regenera para hacer pasar una prueba. Cuando UMB-101/102 sustituyan el visor, el equipo decide si se retira la prueba o se crea un baseline nuevo con su propio origen documentado.
