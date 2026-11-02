# UMBRAL: Asfixia Lúcida

Proyecto final de Gráficas por Computadora: exploración de un sistema solar pequeño desde una nave deteriorada, con un renderizador 3D **por software** escrito en Rust.

> **Estado actual:** pipeline 3D por software, nave pilotable, Sol y tres planetas con órbitas y rotación. Está integrada como opción explícita la composición aprobada del **Sol con ojo**: una criatura texturizada se revela según el oxígeno de depuración y sigue al observador en horizontal. La distribución procedural y el anclaje sobre esferas tienen APIs y vistas técnicas, pero todavía no se aplican a los planetas del juego. **Aún no hay** consumo real de oxígeno, recursos, registros ni identidad propia de los tres planetas.

![Acercamiento a Antena con el overlay técnico](docs/img/overlay.png)

## Requisitos

- Rust y Cargo con soporte de la edición 2024. El manifiesto declara `rust-version = 1.88`; la base se verificó con **Rust 1.97.0 en Windows**. No se ha probado en Linux ni macOS.
- Dependencias fijadas en `Cargo.lock`: `minifb 0.28.0` (ventana y teclado) y `png 0.18.1` (exportar PNG). Ninguna dibuja ni carga geometría. Su aprobación académica sigue **pendiente** (UMB-004).

## Ejecutar

```bash
cargo run --release --locked
```

| Tecla | Acción |
|---|---|
| `W` / `S` | Empuje hacia adelante / frenar y retroceder (suave, depende del tiempo) |
| `A` / `D` o flechas izquierda/derecha | Guiñada |
| Flechas arriba/abajo | Cabeceo |
| `Q` / `E` | Alabeo |
| `Shift` | Impulso ×5 para cruzar el sistema |
| `R` | Reiniciar la nave; en el modo solar, volver a la vista frontal |
| `P` | Guardar el framebuffer actual en PNG |
| `F1` | Mostrar/ocultar el overlay técnico (FPS, contadores de trabajo, interruptores) |
| `F3` | Activar/desactivar el descarte por frustum |
| `F4` | Activar/desactivar el nivel de detalle (LOD) |
| `F5` | Congelar/reanudar las órbitas (y, con `--void-eye`, la respiración de la corona de Vigilia) |
| `F6` | Activar/desactivar el descarte de caras traseras |
| `1`–`4` | Oxígeno de depuración 100 / 50 / 25 / 5 %; controla la revelación en los modos de criatura y, con `--lucid-surface`, las grietas de Antena |
| `M` / `N` | Subir / bajar el oxígeno de depuración en pasos de 5 puntos |
| `7` / `8` | En el modo solar: vista frontal / oblicua |
| `G` / `H` | En el modo solar: objetivo cámara/nave / seguimiento horizontal ON/OFF |
| `Esc` | Salir |

El título de la ventana muestra FPS, velocidad y contadores (triángulos enviados, descartados y rasterizados, fragmentos).

Opciones: `--headless`, `--output archivo.png`, `--frames N` (cierra la ventana tras N frames), `--width N`, `--height N`, `--time S` (tiempo simulado inicial), `--near CUERPO` (arranca junto al cuerpo 0 = Sol, 1 = Antena, 2 = Osario, 3 = Vigilia), `--no-culling`, `--no-lod`, `--overlay` (dibuja el overlay también en `--headless`), `--legacy` (visor ortográfico heredado, donde `A`/`D` rotan la nave), `--help`.

Medición de rendimiento sin ventana: `cargo run --release --example frame_time`.

### Sol con ojo integrado

```bash
cargo run --release --locked -- --creature-model ruta/modelo.obj --creature-texture ruta/textura.png --creature-solar-reveal
```

La criatura usa assets externos que no se incluyen en el repositorio. El flag activa las etapas, la mirada a la cámara y el seguimiento horizontal aprobados; no requiere repetir sus flags individuales. Empieza en la vista frontal con oxígeno al 100 %, el Sol normal y las órbitas vivas. La revelación se prueba con `M`/`N` o los presets `1`–`4`: al 50 % exacto el ojo aún está oculto; aparece al bajar de ese umbral. El Sol visual se desvanece entre 30 y 25 %, sin apagar la iluminación global ni eliminar el cuerpo celeste lógico.

Sin el flag y sin assets externos se conserva el juego base. La composición está aceptada por Charlie; los detalles, restricciones, controles y pruebas están en [`docs/static-creature.md`](docs/static-creature.md#sol-con-ojo-en-el-juego-creature-solar-reveal). No se añade consumo automático de oxígeno ni colisiones.

### Superficie lúcida en Antena

```bash
cargo run --release --locked -- --lucid-surface --near 1
cargo run --release --locked -- --creature-model ruta/modelo.obj --creature-texture ruta/textura.png --creature-solar-reveal --lucid-surface
```

`--lucid-surface` (opt-in) cambia solo el shader del planeta Antena existente a la superficie lúcida aprobada por Charlie (variante 02): grietas rojo-ámbar que aparecen al bajar el oxígeno de 50 %. Con O₂ ≥ 50 % el cuadro es idéntico al juego sin el flag. Material, radio, órbita, rotación, LOD, descarte, cámara, nave y los demás cuerpos no cambian. El O₂ y el tiempo son los del juego (`--debug-oxygen`, `--time`, `1`–`4`, `M`/`N`); no hay consumo automático de oxígeno ni rótulos nuevos.

- `--near 1` arranca junto a Antena; `R` sigue reiniciando la nave como siempre.
- Se combina con el juego normal (también con la criatura estática y su revelación) o con `--creature-solar-reveal`, sin cambiar su composición ni sus teclas.
- Se rechaza con `--legacy` y, fuera del Sol con ojo, con `--creature-eye-compare`, `--creature-distant-turn` o `--creature-horizontal-follow`, antes de leer archivos.

Detalles, contrato y pruebas: [`docs/lucid-surface-preview.md`](docs/lucid-surface-preview.md#integración-en-el-juego---lucid-surface).

### Ojo del vacío en Vigilia

```bash
cargo run --release --locked -- --void-eye --near 3
cargo run --release --locked -- --void-eye --lucid-surface --near 3
cargo run --release --locked -- --creature-model ruta/modelo.obj --creature-texture ruta/textura.png --creature-solar-reveal --void-eye
```

`--void-eye` (opt-in) cambia solo el shader del planeta Vigilia existente al ojo del vacío aprobado por Charlie (UMB-309, variante 02): una pupila negra que mira siempre a la **nave** y una única corona rojo-ámbar que respira lentamente. Material, radio, órbita, rotación, fase, LOD, descarte, cámara, nave y los demás cuerpos no cambian. No depende del oxígeno.

- La respiración sigue el reloj de simulación (`--time`); `F5` la congela, pero el ojo sigue a la nave si esta se mueve. No hay teclas ni rótulos nuevos.
- `--near 3` arranca junto a Vigilia; `R` sigue reiniciando la nave como siempre.
- Se combina con el juego normal (también con la criatura estática y su revelación) o con `--creature-solar-reveal`, en ambos casos con o sin `--lucid-surface`. `G` del Sol con ojo cambia solo la mirada de la criatura, no el objetivo del ojo de Vigilia.
- Se rechaza con `--legacy` y, fuera del Sol con ojo, con `--creature-eye-compare`, `--creature-distant-turn` o `--creature-horizontal-follow`, antes de leer archivos.
- Integración promovida al proyecto principal y verificada por Veliona, con revisión independiente y pruebas de los lanzadores reales. La comprobación física de los controles queda a cargo de Charlie.

Detalles, contrato y pruebas: [`docs/void-eye-preview.md`](docs/void-eye-preview.md#integración-en-el-juego---void-eye).

### Resolución

| Parámetro | Valor |
|---|---|
| Por defecto | **1280×720 (16:9)**, decisión aprobada; definida solo en `src/config.rs` |
| `--width N` / `--height N` | Enteros positivos; cada uno sustituye su valor por defecto de forma independiente |
| Límite | `width × height ≤ 16 777 216` píxeles (`MAX_PIXELS`, equivale a 4096×4096), validado por `Framebuffer::new` antes de reservar memoria, incluido el desbordamiento de la multiplicación |

La ventana y el framebuffer se crean con las mismas dimensiones: no hay escalado ni cambio de tamaño de ventana. Un valor faltante, no entero, cero o demasiado grande termina con código 1 y un mensaje en `Error: ...`, sin escribir PNG ni abrir ventana. Las pruebas de corrección y los renders de regresión no constituyen un benchmark de la escena con la criatura.

- Solo en `--legacy`, la rotación avanza 0.03 rad **por frame**: es el comportamiento heredado. El vuelo del juego depende de `dt`.
- `P` escribe por defecto en `renders/nave.png`, ruta que `.gitignore` no excluye. Para pruebas manuales conviene `--output renders/tmp/vista.png`.
- Los controles se documentan a partir del código; su validación manual en ventana corresponde a Charlie y **no está registrada todavía**.

### Generar imagen sin ventana

```bash
cargo run --release --locked -- --headless --output renders/smoke/frame_720p.png
```

Usa el mismo renderer que la ventana, guarda el PNG a la resolución elegida (por defecto 1280×720) y termina. `renders/smoke/` y `renders/tmp/` están en `.gitignore`.

Para reproducir el **baseline heredado** hay que pedir `--legacy` y 1280×960:

```bash
cargo run --release --locked -- --legacy --headless --width 1280 --height 960 --output renders/smoke/legacy_960p.png
```

El PNG resultante debe ser idéntico byte a byte a `tests/fixtures/legacy_ship_1280x960.png`. El render a 720p es una imagen distinta y no reemplaza ese baseline.

## Validación local

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --release --locked -- --headless --output renders/smoke/frame_720p.png
cargo run --release --locked -- --legacy --headless --width 1280 --height 960 --output renders/smoke/legacy_960p.png
```

Con las dependencias ya descargadas puede añadirse `--offline`. Las pruebas cubren framebuffer y profundidad, rasterización en ambos órdenes de vértices, lector OBJ/MTL con la nave real, exportación PNG, la línea de comandos (resolución por defecto 1280×720 y rechazo de dimensiones inválidas) y una **regresión determinista de la nave**: `tests/ship_baseline.rs` compara el render a 1280×960 píxel a píxel y el PNG headless con `--width 1280 --height 960` byte a byte contra `tests/fixtures/legacy_ship_1280x960.png`. Ese fixture se generó con este repositorio y es idéntico al `renders/nave.png` del laboratorio de origen (SHA-256 `590617750436c32056a02602f94cfb9243e4398599005e94efefa99683e80bfa`).

## Estructura

```text
src/main.rs                 Línea de comandos; delega en app.rs (o en el visor --legacy)
src/app.rs                  Bucle de ventana, entrada y modo headless del juego
src/math/                   Vec2/3/4, Mat4 (MVP, Euler, Rodrigues) y Transform
src/renderer/pipeline.rs    draw_model: vértices, recorte, viewport, rasterización con shader
src/renderer/clipping.rs    Códigos de salida, recorte del plano cercano, Frustum
src/renderer/shader.rs      ShaderKind, ShaderContext, Material, shade_fragment, sRGB
src/renderer/rasterizer.rs  triangle (heredado) y triangle_shaded (perspectiva correcta)
src/renderer/stats.rs       RenderStats: contadores de trabajo
src/renderer/framebuffer.rs Color y z-buffer
src/renderer/image.rs       Exportación a PNG
src/renderer/legacy.rs      Visor heredado, solo con --legacy (baseline de regresión)
src/debug/overlay.rs       Overlay técnico con fuente de mapa de bits propia (F1)
src/scene/                  Cámara, nave, sistema solar y World (update + render)
src/assets/obj.rs           Lector OBJ/MTL: v, vt, vn, caras triangulares, Kd, esfera envolvente
src/assets/primitives.rs    Esfera UV generada por código
assets/models/ship/         OBJ y MTL de la nave
examples/frame_time.rs      Tiempo por cuadro con y sin descarte
tests/                      Pruebas heredadas, transformaciones, pipeline y mundo
docs/architecture.md        Propietarios, contratos y decisiones
```

Distribución procedural reproducible y anclaje orientado de props sobre esfera (UMB-304 y UMB-305, API y vistas técnicas, aún sin integrar en la escena): [`docs/procedural.md`](docs/procedural.md).

Preview técnico de un OBJ con textura PNG en CPU (vista separada del juego, sin integrar): [`docs/textured-preview.md`](docs/textured-preview.md).

Visor interactivo de la criatura con cámara orbital (ventana técnica aislada, sin integrar): [`docs/creature-viewer.md`](docs/creature-viewer.md).

Criatura texturizada dentro del juego: ruta estática, revelación por oxígeno, mirada, seguimiento horizontal y Sol con ojo integrado mediante flags explícitos; sin ellos, el juego no cambia: [`docs/static-creature.md`](docs/static-creature.md).

## Limitaciones conocidas

- Por defecto, los tres planetas comparten esfera y el shader iluminado; solo cambian tamaño, color y órbita. La identidad propia (UMB-302), props, procedural y shaders de Asfixia Lúcida son de Persona 3.
- `AnimatedSun` es una primera versión (UMB-307). `LucidSurface` (superficie lúcida aprobada) solo se usa en Antena con `--lucid-surface`, y `VoidEye` (ojo del vacío aprobado) solo en Vigilia con `--void-eye`.
- No hay colisión, gravedad ni asistencia orbital (Persona 2). La cámara sigue a la nave a distancia fija, con la orientación suavizada y el campo de visión más abierto con la velocidad; no evita atravesar planetas.
- El LOD solo cubre las esferas de los cuerpos (tres niveles por tamaño aparente, con histéresis del 15 %); los props de Persona 3 aún no lo usan.
- El recorte se hace solo contra el plano cercano; los demás lados los resuelve la caja de pantalla del rasterizador (verificado con un triángulo de 2000 unidades que cruza la cámara).
- El descarte de objetos reduce vértices procesados, pero a 720p el tiempo lo domina el relleno de fragmentos; el efecto en milisegundos es pequeño.
- La ruta de los assets se fija con `CARGO_MANIFEST_DIR` al compilar: el binario no es reubicable.
- El baseline de la nave solo se ha verificado en Windows x86_64.
- El descarte de caras traseras se aplica a planetas y nave (la malla de la nave está cerrada y orientada hacia afuera, comprobado por prueba).
- El orden y sentido de la nave (`SHIP_MODEL_YAW`) se ajustó mirando un render; no hay prueba automática.

## Origen y atribución

- Código base, pruebas y modelo: [`carga-modelos-24531`](https://github.com/SrCharlied/carga-modelos-24531), commit `51fbaed7d6c5db12a264ce91b86f450e7b05dd34`, autor SrCharlied según su historial. El historial Git de ese laboratorio no se importó; esta referencia sustituye a la atribución por historial.
- Origen y hashes de la nave: `assets/models/ship/README.md`.

## Contribuciones

| Quién | Aporte en este repositorio |
|---|---|
| SrCharlied | Laboratorio base (código, pruebas, modelo) y dirección del proyecto |
| Persona 1, 2, 3 | Asignación pendiente; ver responsabilidades en `docs/architecture.md` |

## Uso de asistencia de IA

- **UMB-001/UMB-002** (importación, renombre, reorganización de módulos, prueba de regresión de la nave y esta documentación inicial) se realizaron con **Claude Code** (Anthropic) bajo dirección de Charlie, con planificación y revisión apoyadas por un asistente de IA local. La revisión humana de este trabajo está pendiente.
- La integración de la criatura, su seguimiento y el Sol con ojo se realizó con asistencia de Claude Code y supervisión del asistente local; Charlie aprobó la composición y el resultado integrado. Las pruebas automatizadas y revisiones de código no sustituyen un registro pormenorizado de controles físicos.
- El uso de IA en el laboratorio de origen no está declarado en su README; debe completarlo su autor.
