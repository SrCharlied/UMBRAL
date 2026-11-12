# Arquitectura de UMBRAL

Estado del árbol: renderer CPU y escena jugable integrados. `src/gameplay/` contiene el estado de Persona 2 y `World` coordina simulación y dibujo. Los contratos descritos reflejan el código actual; la aceptación formal entre integrantes (UMB-003) sigue pendiente. La consulta sobre bibliotecas permitidas (UMB-004) también espera respuesta del profesor.

## 1. Flujo actual

```text
OBJ + MTL -> assets::obj -> Model
                          |
 app.rs: ventana/entrada o headless, calcula dt
                          |
 World::update: GameplayState, nave, navegación, colisiones,
                recolección y cámara
                          |
 World::render: estrellas, cuerpos, balizas y nave
                          |
 draw_model -> transformaciones -> recorte cercano -> rasterizado
             -> z-buffer / shader CPU -> Framebuffer
                          |
 minifb presenta buffer | image::save_png exporta PNG
```

`--legacy` selecciona el visor ortográfico heredado de `renderer/legacy.rs`; ahí `A`/`D` giran 0.03 radianes por frame. El juego usa `dt` para avance y respuesta de nave. `--help` es básico y está desactualizado; su actualización queda pendiente y no debe tomarse como inventario fiable de opciones.

## 2. Módulos y responsabilidades presentes

| Módulo | Responsabilidad actual |
|---|---|
| `src/main.rs` | Analiza argumentos y selecciona aplicación de juego o visor `--legacy`. |
| `src/app.rs` | Bucle de ventana y headless, entrada, `dt`, acciones de gameplay y feedback textual mínimo. |
| `src/math/` | Tipos propios `Vec2`, `Vec3`, `Vec4`, `Mat4` y `Transform`; ya existen y los consumen escena y renderer. |
| `src/scene/` | Cámara, nave, sistema solar y `World` con `update` y `render`. |
| `src/gameplay/` | `GameplayState`, oxígeno, recursos, registros, navegación, asistencia, gravedad opcional y colisiones. |
| `src/renderer/` | Framebuffer y profundidad, pipeline, recorte, rasterizador, shaders CPU, estadísticas, PNG y visor heredado. |
| `src/debug/` | Overlay técnico existente. |
| `src/assets/` | Lector OBJ/MTL y primitivas generadas por código. |
| `tests/` | Regresiones del renderer/base, escena y gameplay. |

La escena dibuja los cuerpos, balizas de recursos y registros, nave y estrellas. El feedback de gameplay usa la fuente disponible; no es el HUD artístico que corresponde a Persona 3. Los modelos, shaders específicos, contenido procedural y dirección artística de Persona 3 permanecen como trabajo pendiente.

## 3. Contratos de ejecución

- Unidades: `f64`, ángulos en radianes, tiempo en segundos y distancias en unidades de mundo.
- `Transform` transforma posición, rotación y escala de mundo. La convención implementada es mano derecha, Y arriba, cámara hacia −Z, NDC `[-1,1]`, `R = Ry·Rx·Rz` y `M = T·R·S`. La convención está implementada pero su firma de equipo sigue pendiente.
- `draw_model` no limpia el framebuffer ni autoencuadra. El rasterizador usa coordenadas de pantalla, profundidad y atributos con corrección de perspectiva en `triangle_shaded`.
- `ShaderContext` lleva tiempo, oxígeno visual y posiciones de cámara, nave y luz. `World::oxygen01()` es el valor visual, que puede tener override; `World.gameplay.oxygen01` conserva el oxígeno real.
- `GameplayState` existe y mantiene oxígeno normalizado, conteos, estados terminales, ajustes, navegación, catálogos y eventos. El agotamiento termina la partida; reunir tres registros habilita la condición de llegada al Sol.
- `World::update(input, dt)` integra gameplay y mundo. Pasos no finitos, no positivos o mayores a 60 s no son integrados por la nave y navegación. Para `dt > 60`, sin embargo, el gameplay puede consumir oxígeno y el reloj orbital de `World` puede avanzar; la recolección aún se evalúa con nave fija y anclas en ambos tiempos, por lo que oxígeno y órbitas pueden avanzar mientras navegación ignora ese paso. Este es un límite conocido, no una garantía de simulación equivalente a pasos pequeños.
- La asistencia `O` puede ser interrumpida temporalmente por entrada manual y recapturar al soltarla mientras siga habilitada. Pulsar `O` desactiva la asistencia persistente. `R` reinicia; `1`–`4` fuerzan solo oxígeno visual y `0` limpia el override.
- Colisiones son esféricas con barrido lineal relativo entre posiciones de dos cuadros, no física orbital completa. La recolección aproxima el movimiento de anclas orbitales por un segmento entre los tiempos del cuadro.
- `minifb` presenta el buffer y recibe teclado; `png` codifica capturas. Render, rasterización, z-buffer, shaders y matemáticas son propios.

## 4. Decisiones y pendientes

| Tema | Situación |
|---|---|
| Firma de contratos UMB-003 | Convenciones y APIs implementadas; aceptación de las personas afectadas pendiente. |
| Bibliotecas UMB-004 | Consulta al profesor pendiente. Dependencias fijadas: `minifb 0.28.0` y `png 0.18.1`; matemáticas propias. |
| Asignación formal de Personas 1, 2 y 3 | Pendiente del equipo. La atribución de contribuciones existentes se conserva en README y `docs/persona2.md`. |
| Assets y baseline | Assets resueltos desde `CARGO_MANIFEST_DIR`; baseline heredado verificado solo en Windows x86_64. Orientación del OBJ ajustada visualmente mediante `SHIP_MODEL_YAW`. |
| Escalado y resize | Ventana y framebuffer usan el mismo tamaño; escalado entre resoluciones y ventana redimensionable no están implementados. |
| CLI | `--help` existe, pero es básico/desactualizado y queda pendiente actualizarlo. |
| Presentación | Pendientes pilotaje humano, video y validación visual. No se afirma que las pruebas automatizadas sustituyan esas evidencias. |

## 5. Validación y restricciones

En la verificación posterior de esta reanudación, el coordinador informó que pasaron `cargo test --locked --no-fail-fast` y `cargo clippy --all-targets --locked -- -D warnings`; el Clippy estricto que falló históricamente ya pasa en esa comprobación del coordinador. `cargo fmt --check` también pasó en esta reanudación tras aplicar `rustfmt` explícito con `skip_children=true` a la lista permitida. Sigue pendiente comprobar desde un clon limpio.

El coordinador generó fuera del repositorio `scratch/umbral-persona2-prepublish.png` con ejecución headless a 640×360 junto al cuerpo 1; se observa legible el feedback inferior. La captura no acredita narrativa completa ni pilotaje humano. La revisión independiente reportó JSON `passed: true`, sin defectos bloqueantes.

La respuesta del profesor sobre bibliotecas permanece pendiente (UMB-004). La postura actual usa `minifb` para ventana/teclado/presentación, `png` para exportación, matemáticas propias y renderer CPU propio; no incorpora biblioteca 3D, shaders GPU ni física externa.

## Origen y atribución

El código base, pruebas y modelo proceden de [`carga-modelos-24531`](https://github.com/SrCharlied/carga-modelos-24531), commit `51fbaed7d6c5db12a264ce91b86f450e7b05dd34`, autor SrCharlied según su historial. El historial Git de ese laboratorio no se importó. Origen y hashes de la nave: `assets/models/ship/README.md`.
