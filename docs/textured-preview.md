# Preview texturizado en CPU

**Qué es:** una vista técnica sin ventana que dibuja un OBJ con su textura PNG mediante el **mismo pipeline del juego** (`draw_model_with`) y un shader texturizado. Sirve para ver el modelo original de la criatura con su textura y prepara una extensión mínima del renderer.

**Qué no es:** no integra la criatura en la escena, no cambia los shaders existentes, no anima ni implementa LOD. Ningún valor por defecto de la escena usa esta ruta.

## Uso

```bash
cargo run --release --example creature_preview -- \
    --model ruta/al/modelo.obj --texture ruta/a/textura.png --output salida.png \
    [--unlit] [--yaw RAD] [--benchmark]
```

- **Rutas obligatorias:** `--model`, `--texture` y `--output`. El example no tiene ninguna ruta fija.
- **El OBJ** se carga con `load_model` actual, así que su MTL debe estar junto a él. Su `map_Kd` **no** se lee: la textura se pasa explícitamente con `--texture`.
- **Iluminación:** `Lit` por defecto; `--unlit` muestra el albedo sin iluminación.
- **`--yaw`:** gira el modelo en Y alrededor de su centro envolvente (`bounds_center`), en radianes.
- **`--benchmark`:** hace 2 calentamientos y 11 mediciones del mismo cuadro, e imprime la mediana (índice 5 de los tiempos ordenados).
- **Resolución:** la del juego por defecto (`config`, 1280×720).
- **Encuadre:** la esfera envolvente del modelo (`bounds_center`, `bounds_radius`) cabe en el FOV vertical de 40° con un margen del 10 %. La cámara se coloca en el lado +Z del modelo, ligeramente elevada; la luz va arriba, a la izquierda y por delante. La imagen lleva el rótulo «VISTA TECNICA».

El asset original de la criatura **no está en Git**: vive en la carpeta local de inspección y se pasa por CLI.

## Piezas

| Pieza | Archivo | Contrato |
|---|---|---|
| `Texture` | `src/renderer/texture.rs` | RGB de 8 bits compacta (3 bytes por texel, en el orden de filas del PNG). Ver formatos y límites |
| `srgb_to_linear` | idem | Tabla de 256 entradas con la curva sRGB estándar (IEC 61966-2-1) |
| `Texture::sample(uv) -> Option<Color>` | idem | Bilineal con *clamp* y V invertida. `None` solo si alguna coordenada es NaN |
| `draw_model_with(fb, model, transform, material, camera, shade)` | `src/renderer/pipeline.rs` | Punto de extensión opt-in: `shade(fragmento, material_de_la_cara)` colorea cada fragmento que pasa el z-test. `draw_model` es un envoltorio de una línea sobre esta función |
| `TexturedShader` | `src/visual/textured.rs` | Modos `Unlit` (albedo) y `Lit` (difuso + ambiente) |

### Formatos y límites de textura

- **Solo PNG RGB de 8 bits.** RGBA, gris, gris+alfa, paleta y 16 bits se rechazan con «formato PNG no admitido». No hay ruta de alfa: la textura de la criatura no la tiene.
- **Cada lado debe estar en `1..=4096`** (`MAX_TEXTURE_SIDE`).
- **La validación usa solo el IHDR** (`read_header_info`), antes de preparar la lectura y de reservar el buffer de imagen (como mucho 48 MiB). Una cabecera que declara 100000×100000 se rechaza sin reservar nada.
- **El decodificador `png` corre con `Transformations::IDENTITY`** y `Limits { bytes: 64 MiB }` (`PNG_DECODER_LIMIT`) para sus buffers internos.
- **Un PNG corrupto, truncado o ilegible** devuelve `Err` («PNG invalido: …»). Si falta el archivo, el error nombra la ruta.

### Muestreo

- **Convención OBJ:** `v = 0` es la parte de abajo de la imagen y la fila 0 del PNG es la de arriba (`fila = (1 − v)·alto − 0.5`). Lo confirman la prueba con el fixture 2×2, la del cuadrado en pantalla y la evidencia estadística de la auditoría.
- **Interpolación bilineal entre centros de texel,** con *clamp* al borde. Los UV fuera de `[0, 1]`, incluidos `±∞`, toman el texel del borde. No hay repetición: los UV de la criatura están dentro de `[0, 1]`.
- **Cada texel se convierte de sRGB a lineal antes de interpolar.** Interpolar en sRGB y convertir después daría colores distintos, y lo comprueba una prueba.
- **UV con NaN:** `sample` devuelve `None` y el shader pinta `INVALID_UV_COLOR` (magenta) para que el problema sea visible. No se inventa un color plausible.
- **No hay mipmaps ni antialiasing.** Con la criatura a este tamaño, la minificación es fuerte (en la auditoría, unos 23 texeles por fragmento), así que es esperable que aparezcan aliasing y parpadeo al moverse. Es una decisión pendiente; no se promete antialiasing.

### Shader texturizado

- **`Unlit`:** devuelve `albedo` (el color lineal muestreado).
- **`Lit`:** devuelve `albedo · (ambiente + (1 − ambiente) · max(0, n·l))`.
  - `n` es la normal interpolada, renormalizada.
  - `l` es la dirección desde el punto hacia `light_position`.
  - El ambiente está en `[0, 1]` y la luz tiene que ser finita; si no, `new` devuelve `Err`.
- **El `Kd`/`base_color` del material se ignora** (tinte blanco). El MTL de la criatura no trae `Kd` y el valor por defecto (0.7) oscurecería la textura en silencio; una prueba lo vigila.
- **La textura se captura una sola vez al construir el shader.** `shade` no carga ni reserva memoria por fragmento.
- **Sin *backface culling* en el preview** (`Material::from_model()`): la malla de la criatura no es cerrada (25 aristas de borde según la inspección en Blender).
- **No usa el patrón por UV de `LitPlanet`.**

## Pruebas

- **`tests/texture.rs`:**
  - Curva sRGB y monotonía.
  - Esquinas con V invertida y bilineal en espacio lineal.
  - *Clamp* con `±∞`, `None` con NaN y textura de 1×1.
  - Dimensiones y bytes incoherentes.
  - Decodificación de un RGB8 sintético; rechazo de RGBA, gris, gris+alfa, paleta y 16 bits.
  - 4097×1 real y cabecera falsa de 100000×100000.
  - PNG corrupto, truncado, basura y ruta inexistente, incluido el CRC de `IEND` y la ausencia de ese chunk. El lector termina con `finish()` para validar el final del PNG.
- **`tests/textured_pipeline.rs`:**
  - `draw_model` y `draw_model_with` con `shade_fragment` dan píxeles idénticos y **todos** los contadores iguales. Se comprueba con la nave y una esfera superpuestas (z-test), 3 shaders × 2 materiales (uno con *culling*).
  - El callback recibe UV con corrección de perspectiva, comparada con una intersección rayo–plano independiente.
  - El shader: albedo en `Unlit`, fórmula difusa en `Lit`, magenta con NaN, rechazo de parámetros, filas del PNG en su sitio en pantalla y `Kd` 0.7 ignorado.
- **Los fixtures son sintéticos y pequeños**, generados en las propias pruebas. No dependen del asset de la criatura.
- **Pruebas internas del example** (`cargo test --example creature_preview`, incluidas también con `cargo test --all-targets`): el giro conserva el centro envolvente y el radio de un modelo descentrado; yaw 0 conserva exactamente el transform anterior.

## Medición (1280×720, criatura original, cámara del example, yaw 0)

31 096 triángulos rasterizados, 0 descartados, 223 126 fragmentos. 2 calentamientos y 11 mediciones por modo, en serie y sin otros trabajos, build release en la máquina de desarrollo (Windows):

| Modo | Mediana | Mín–máx |
|---|---|---|
| `Unlit` | 67.2 ms | 61.8–71.7 ms |
| `Lit` | 68.5 ms | 63.3–75.6 ms |

- **Carga:** OBJ ~30 ms; PNG 4096² ~210–220 ms (una sola vez).
- **Esto no es el FPS del juego:** es un solo modelo, sin escena y con esta cámara. La diferencia medida entre `Lit` y `Unlit` es pequeña; esta comparación no es un perfil que aísle el coste de la geometría y el rasterizado del muestreo de textura compartido, ni permite atribuir qué coste domina.

## Pendiente

- **Inspección visual de Charlie** antes de elegir una ruta definitiva. La revisión técnica de Veliona distingue ojo, aro y tentáculos con la textura original; no implica aprobación artística ni igualdad píxel a píxel con Blender, cuya cámara e iluminación son distintas.
- **Mipmaps o nivel prefiltrado:** el preview con la textura original 4096 muestra granulado. Un experimento con una copia 1024 prefiltrada mediante promedio en espacio lineal lo reduce; es solo diagnóstico, no mipmapping automático ni una variante elegida por defecto. El original permanece intacto.
- **Integración en la escena** (Material o textura en el contrato de Persona 1, lectura de `map_Kd`), **LOD y animación.** Nada de esto está hecho.
- **Origen del modelo:** es un modelo externo generado con Meshy AI. Su uso y su declaración dependen de la rúbrica y de los términos de Meshy, que no se han verificado aquí.
