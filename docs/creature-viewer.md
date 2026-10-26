# Visor interactivo de la criatura (CPU)

**Qué es:** una ventana técnica de 1280×720 que muestra un OBJ con su textura PNG y deja orbitar una cámara alrededor de su centro envolvente. La criatura queda **estática**; solo se mueve la cámara. Dibuja con el **mismo render** que el modo sin ventana: `render_view`, que llama a `draw_model_with` con `TexturedShader` (ver [`textured-preview.md`](textured-preview.md)).

**Qué no es:** no integra la criatura en el juego ni cambia sus valores por defecto. Tampoco anima, no tiene ojo que siga a la nave, ni mipmaps, LOD, ratón, paneles, redimensionado ni calidad adaptativa.

## Uso

```bash
# Ventana
cargo run --release --example creature_viewer -- \
    --model ruta/al/modelo.obj --texture ruta/a/textura.png \
    [--output captura.png] [--frames N] [--yaw RAD] [--pitch RAD] [--distance RADIOS] [--unlit]

# Sin ventana (imagen determinista y medición)
cargo run --release --example creature_viewer -- \
    --model ruta/al/modelo.obj --texture ruta/a/textura.png \
    --headless --output salida.png [--benchmark] [--yaw RAD] [--pitch RAD] [--distance RADIOS] [--unlit]
```

| Opción | Contrato |
|---|---|
| `--model`, `--texture` | Obligatorias. El MTL del OBJ debe estar junto a él; su `map_Kd` no se lee. La textura tiene que ser PNG RGB de 8 bits (ver el contrato de `Texture`). Cada archivo se carga una sola vez |
| `--yaw RAD` | Cualquier valor finito; se envuelve a `[−π, π]`. `0` mira desde +Z del OBJ (la vista frontal del preview estático) |
| `--pitch RAD` | Finito, dentro de `[−1.4, 1.4]` (`MAX_PITCH`, unos 80°). Por defecto `atan(0.15)`, igual que el preview estático |
| `--distance RADIOS` | Distancia de la cámara al centro, en radios de la esfera envolvente. Debe estar en `[1.25, 12]` (siempre fuera de la esfera). Por defecto `1.1 / sin(20°) ≈ 3.22`, el encuadre del preview estático |
| `--unlit` | Arranca en `Unlit` (albedo). Por defecto `Lit` |
| `--headless` | No abre ventana: dibuja un cuadro, le añade el HUD sin tiempos y escribe `--output`, que es obligatorio en este modo. Las métricas salen por stdout |
| `--benchmark` | Solo con `--headless`. Hace 2 calentamientos y 11 mediciones de `render_view` con la misma pose (limpiar y dibujar), sin HUD ni escritura del PNG, e imprime la mediana |
| `--frames N` | Solo en ventana, `N ≥ 1`. Cierra la ventana tras N cuadros presentados (para un *smoke* corto). Si además se pasa `--output`, guarda el último cuadro al salir |
| `--output archivo.png` | En ventana es opcional: es el destino de la tecla `P` (cada captura sobrescribe el mismo archivo) y del PNG final de `--frames` |

**Validación previa:** toda la línea de comandos se valida **antes** de leer archivos, abrir la ventana o escribir nada. Terminan con `Error: …` y código 1:
- `--frames 0`, un ángulo NaN o infinito, un pitch o una distancia fuera de rango.
- Una opción desconocida o sin valor.
- Combinaciones inválidas: `--headless` sin `--output`, `--frames` con `--headless` o `--benchmark` sin `--headless`.
- Un `--output` que no termina en `.png`, o que resuelve (`canonicalize`) a la misma ruta que `--model` o `--texture`.

No hay rutas fijas ni assets ocultos: el asset de la criatura no está en Git y se pasa por CLI.

## Controles (ventana)

| Tecla (`minifb::Key`) | Acción |
|---|---|
| `Left` / `Right` | Orbitar en yaw. Con la derecha, la cámara se desplaza hacia su derecha (a yaw 0, hacia +X) |
| `Up` / `Down` | Subir / bajar la cámara (pitch), saturado en `±MAX_PITCH`: nunca cruza los polos |
| `W` / `S` | Acercar / alejar. La distancia se multiplica por `e^(∓dt)` y se satura en `[1.25, 12]` radios |
| `R` | Volver **exactamente** a la pose de arranque (la de la CLI o la de por defecto) |
| `L` | Alternar `Lit` / `Unlit` |
| `P` | Capturar el cuadro actual, HUD incluido, en `--output`. Sin `--output`, avisa por stdout y sigue |
| `Escape` | Salir |

- **Velocidades:** yaw 1.5 rad/s y pitch 1.0 rad/s. Las teclas opuestas pulsadas a la vez se anulan.
- **Lectura de teclas:** `R`, `L` y `P` usan `is_key_pressed(…, KeyRepeat::No)`; las de movimiento, `is_key_down`.
- **Los nombres de las teclas** se comprobaron en el enum `Key` de `minifb 0.28.0` instalado. **No hay prueba física de teclado:** está pendiente la prueba manual de Charlie.

## Estado y contratos (`src/visual/creature_viewer.rs`)

| Pieza | Contrato |
|---|---|
| `OrbitPose` | Solo se construye validada (`new` → `Result`). Yaw envuelto (exacto dentro de `[−π, π]`), `|pitch| ≤ MAX_PITCH` y distancia en `[MIN_DISTANCE_RADII, MAX_DISTANCE_RADII]`. `direction()` es unitario |
| `OrbitInput` | Ejes en `[−1, 1]`: `yaw` (+ derecha), `pitch` (+ arriba), `zoom` (+ acercar). `from_keys` traduce las teclas |
| `OrbitController::step(input, dt)` | `dt` debe ser finito y no negativo, y se recorta a `MAX_DT = 0.1 s` para que una pausa no provoque saltos. Ejes fuera de rango o NaN devuelven `Err`. **Ante cualquier `Err` el estado no cambia** |
| `OrbitController::reset` | Restablece la pose inicial bit a bit |
| `CreatureBounds` | Centro con componentes en `[−1e9, 1e9]` y radio en `[1e-6, 1e9]`. Además, el radio debe ser ≥ `1e-9 ×` la mayor componente del centro, para que la precisión de `f64` alcance |
| `CreatureBounds::camera(pose, aspect)` | FOV de 40°. `near = 0.01·d` y `far = 4·d` con `d = distancia·radio`: el plano cercano queda delante de la esfera y el lejano detrás. Devuelve `Err` si el aspecto es inválido o si la posición o la matriz de vista-proyección no son finitas. Usa `Camera` y `Mat4` existentes sin modificarlos |
| `render_view` | Valida cámara y shader, **después** limpia el framebuffer y dibuja con transform identidad, `Material::from_model()` (sin descarte de caras traseras: la malla no es cerrada) y `TexturedShader`. Ambiente 0.25; luz fija en el mundo en `centro + (−1.5, 2, 3)·radio`, como el preview estático. Con `Err` no toca el framebuffer |
| `hud_lines` | Modo, tamaño real de la textura, distancia en radios e instrucciones. Con tiempos (solo en ventana) añade «RENDER CPU … MS \| PRESENTADOS … FPS (TOPE 60)»; un valor no finito se muestra como `--` |
| `validate_output_path` | Exige `.png` y que no coincida con ninguna entrada tras resolver la ruta |

El estado es **solo de cámara**: `render_view` recibe `&Model` y nunca transforma ni modifica la criatura.

### HUD y tiempos

- **Título de la ventana:** estático («UMBRAL - visor tecnico de criatura»). Las métricas no se repiten en él.
- **Reloj del HUD:** `Instant` monotónico independiente del `dt` del controlador. La primera ventana empieza antes del bucle; cada cuadro se cuenta con un timestamp tomado **después** de `update_with_buffer`. Al alcanzar al menos 0.5 s (`HUD_PERIOD`), se publica la muestra y se reinician reloj y acumuladores en **ese mismo timestamp**, sin arrastrar excedentes ni descartar el coste del cuadro actual.
- **Render CPU:** media de `render_view` (limpiar y dibujar) de exactamente los cuadros contados en esa ventana. No incluye el HUD, la captura ni la presentación.
- **Presentados FPS:** cuadros mostrados divididos por la duración real de esa misma ventana (puede superar 0.5 s). Incluye render, HUD, captura y presentación de `minifb` (`update_with_buffer`), que **espera** para respetar el tope de 60 FPS (`set_target_fps`). Si el render tarda más que el tope, la espera es nula. Una captura lenta reduce los FPS de su intervalo, pero no aumenta la media de render CPU.
- **Al cerrar,** stdout resume los cuadros presentados, el render medio y la presentación media.
- **En `--headless`,** el HUD no incluye tiempos: la imagen depende solo de la pose, el modo y las entradas.

## Pruebas

**`tests/creature_viewer.rs`** (13; fixtures sintéticos, no necesita el asset ni abre ventanas):
- **Pose por defecto:** coincide con el encuadre del preview estático.
- **Validación de poses:** rechaza NaN, `±inf` y valores fuera de rango; el envoltorio del yaw es exacto dentro de `[−π, π]`.
- **Flechas:** signo y velocidad documentados; el yaw se mantiene envuelto.
- **Pitch:** saturado sin cruzar los polos.
- **`W`/`S`:** acercamiento exponencial y reversible; la distancia queda saturada y siempre fuera de la esfera.
- **`dt` y entradas:** los valores inválidos dan `Err` sin cambiar el estado; las pausas se recortan a `MAX_DT`.
- **Reset:** restablece la pose bit a bit.
- **Centro desplazado:** con centro en `(1e6, −3e5, 2.5e5)` y radio 0.5, y con radio 40 en el origen, la cámara queda a `d·r`, mira al centro, el centro se proyecta en el centro de la imagen y near/far rodean la esfera.
- **Contrato de bounds:** rechaza valores inválidos; en los extremos válidos, cámara y matriz son finitas.
- **HUD:** determinista sin tiempos; separa render y presentación; usa solo caracteres de la fuente y cabe en 1280 px.
- **`--output`:** debe ser `.png` y no puede ser una entrada (incluida la variante con `./`).
- **Mismo pipeline:** `render_view` produce píxeles y contadores idénticos a una llamada directa a `draw_model_with` con la cámara del visor y `TexturedShader`, y limpia un framebuffer sucio.
- **Imágenes por pose:** con un cubo texturizado de centro desplazado, 6 poses dan imágenes distintas y repetibles. En cada píxel, profundidad finita equivale a píxel pintado, con la profundidad en `[−1, 1]`. `Lit` y `Unlit` difieren, el centroide queda en el centro de la imagen y más cerca se ve más grande.

**Pruebas del example** (`cargo test --example creature_viewer`, incluidas en `--all-targets`): valores por defecto, reproducción exacta de la pose por CLI y 17 argumentos inválidos rechazados por el parser.

### Validado y pendiente

El acumulador privado de métricas del example se prueba sin ventanas con timestamps post-presentación inyectados: primera ventana con cuadros lentos, duraciones variables, pausa por captura y reinicio exacto del reloj y de los cuadros/render acumulados. El bucle usa ese mismo helper y le pasa `Instant::now` como reloj después de presentar.

- **Validado por código:** el estado y la cámara (pruebas), el render headless sintético (pruebas) y el modelo real por CLI en headless. Las 8 poses dieron 8 imágenes distintas y la repetición fue idéntica byte a byte. Los errores de CLI terminan con código 1 sin crear archivos.
- **Verificación final del supervisor:** 114 tests con `--all-targets`, formato y Clippy estricto correctos; las 8 referencias headless anteriores y las 6 imágenes del visor se conservan tras corregir las métricas. El lanzador se ejecutó otra vez sin ventana y con una ventana nativa de 16 cuadros, cierre automático y HUD numérico visible, sin teclas automatizadas. La revisión independiente final no encontró bloqueos. Esta comprobación no sustituye la prueba manual de los controles.
- **Pendiente, manual de Charlie:** respuesta física y sensación de los controles, y legibilidad del HUD y sus métricas en vivo. El smoke de apertura y cierre no valida esa interacción.

## Medición (1280×720, criatura original de 31 096 triángulos, `render_view`)

Build release en Windows, en serie y sin otros trabajos. 2 calentamientos y 11 mediciones por fila; no incluye el HUD ni la escritura del PNG:

| Textura | Modo | Distancia | Fragmentos | Mediana | Mín–máx |
|---|---|---|---|---|---|
| Candidata 1024² | Lit | 2 radios | 615 348 | 100.2 ms | 94.7–109.4 ms |
| Candidata 1024² | Lit | ≈3.22 (por defecto) | 223 126 | 52.8 ms | 50.9–57.7 ms |
| Candidata 1024² | Lit | 6 radios | 61 565 | 30.6 ms | 28.6–37.7 ms |
| Candidata 1024² | Unlit | ≈3.22 | 223 126 | 47.5 ms | 45.8–49.2 ms |
| Original 4096² | Lit | ≈3.22 | 223 126 | 80.1 ms | 70.9–94.3 ms |

- **Carga:** OBJ de 33 a 59 ms; PNG de 1024², de 15 a 18 ms; PNG de 4096², 253 ms.
- **Esto no es el FPS del juego ni un perfil.** El tiempo crece con los fragmentos de la pose, pero estas mediciones no aíslan qué etapa domina.
- **Variación entre sesiones:** la fila de 4096² dio 68.5 ms en la medición del preview estático y 80.1 ms aquí, con la misma pose y la misma cantidad de fragmentos. Hay que comparar dentro de una misma sesión.

## Pendiente

- **Prueba manual de Charlie** en ventana: respuesta física y sensación de los controles, legibilidad del HUD en vivo.
- **Decisión sobre la textura:** la candidata 1024 prefiltrada está aceptada para seguir con las pruebas; no es la opción por defecto ni tiene mipmapping automático.
- **Integración en el juego, animación, LOD y mipmaps:** nada de esto está hecho.
- **Origen del modelo:** es externo, generado con Meshy AI. Su uso y su declaración dependen de la rúbrica y de los términos de Meshy, que no se han verificado aquí.
