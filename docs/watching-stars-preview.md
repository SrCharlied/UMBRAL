# Estrellas observadoras: preview aislado (UMB-310)

**Estado:** preview AISLADO para que Charlie decida. No está integrado en el juego: sin flag, App, World, CLI ni exports en `lib.rs`/`visual/mod.rs`. El módulo `src/visual/watching_stars.rs` se compila por `#[path]` desde `tests/watching_stars.rs` y `examples/watching_stars_preview.rs`.

**Dirección de Charlie** (literal): «al 60% empieces a ver una o 2 a la distancia, osea que la generación de esas estrellas que te ven sea muy baja / lueego, tipo 50% empiezan a aprecer más / luego 25% es mucho más lleno y ya directamente casi todas las estrellas te observan».

**No incluye** nuevos modelos, HUD, audio, combate, ataques, consumo automático de O₂, optimizaciones ni parpadeo, pulsos o strobe. No cambia la percepción aprobada ni ningún algoritmo aceptado.

## Hipótesis: «Pupilas de alfiler»

Las mismas 700 estrellas del cielo publicado (`draw_stars` del World) abren, al bajar el O₂, un ojo diminuto cuya pupila mira a la nave proyectada:
- **Mismo cielo:** mismos IDs, direcciones, colores, píxel y orden de dibujo. No se genera otro cielo ni se mueven estrellas, y no hay parallax por traslación de la cámara.
- **Tamaño fijo en píxeles,** como la estrella de 1 px: lejano, no una cara. Cada ojo se dibuja sobre el píxel de su estrella.
- **Sin animación propia:** para un O₂ dado, el ojo es estático.

## API (pura)

```rust
render_watching_stars(&Framebuffer, &Camera, ship_position: Vec3, oxygen01: f64)
    -> Result<WatchingStars { pixels: Vec<u32>, report: WatchingStarsReport }, String>
```

- Recibe el cuadro **ya dibujado**, con su profundidad, y devuelve píxeles nuevos. La fuente (píxeles y profundidad) no cambia.
- **Con O₂ ≥ 65 %** (`OBSERVER_ONSET_OXYGEN`) los píxeles son EXACTOS y no hay ninguna observadora.
- **Informe:** `oxygen01`, `density` (peso medio global) y las listas:
  - `active_ids`: peso > 0 en toda la esfera;
  - `projected_ids`: en cuadro y estrella visible de su píxel;
  - `occluded_ids`: geometría delante del centro;
  - `shadowed_ids`: otra estrella anterior ganó ese píxel en `draw_stars`;
  - `eyes`: ID, rank, píxel, peso, desplazamiento de pupila y píxeles cambiados;
  - `ship_focus_px`.
- **Funciones auxiliares públicas:** `star_catalog`, `sky_view_projection`, `star_pixel`, `observer_ranking`, `observer_thresholds`, `observer_weights` y `observer_density`.

**Un conteo de política no equivale a ojos legibles.** `eyes` es lo que se dibujó; tras el desenfoque y el túnel de la percepción, un ojo dibujado puede no leerse.

## Selección: ranking fijo y curva candidata

**Ranking** (`observer_ranking`): muestreo del punto más lejano sobre las 700 direcciones.
- Empieza en el ID 0; los empates se resuelven por ID; no hay semilla aleatoria.
- Cada observadora nueva es la estrella más alejada de las ya elegidas. Así, las primeras quedan repartidas por la esfera (las 16 primeras están a ≥ 43° entre sí) y cualquier encuadre ve aproximadamente la misma fracción.
- Con un hash uniforme, el encuadre de arranque no tenía ninguna de las primeras 25.
- No depende de la cámara ni del cuadro, y es acumulativo y reversible: el peso es función pura del O₂.

**Peso por estrella:** la estrella de rank `k` tiene un umbral `θ_k`. Su peso es `smoothstep((θ_k − O₂) / 0,03)`: empieza a abrir al bajar de su umbral y queda abierta del todo un 3 % de O₂ después. Los umbrales siguen una **Hermite monótona (PCHIP)** por estos nodos (fracción de ranking → O₂):

| Fracción de ranking | 0 | 28/700 | 0,24 | 0,96 | 1 |
|---|---|---|---|---|---|
| Umbral de O₂ | 0,65 | 0,60 | 0,50 | 0,25 | 0,10 |

**Curva candidata,** declarada como tal; Charlie no fijó porcentajes para el 50 y el 25 %. Valores globales sobre las 700:

| O₂ | Peso > 0 | Legibles (peso ≥ 0,5) | Peso medio |
|---|---|---|---|
| 100 / 70 % | 0 | 0 | 0 (cuadro exacto) |
| 60 % | 28 (4 %) | 18 (2,6 %) | 0,026 |
| 55 % | 84 | 64 | 0,092 |
| 50 % | 168 | 142 (20 %) | 0,203 |
| 40 % | 460 | 413 | 0,588 |
| 25 % | 672 | 659 (94 %) | 0,942 |
| 10 % | 700 | 697 | 0,996 |
| 0 % | 700 | 700 | 1 |

- **Banda técnica por encima del 60 %:** entre 65 y 60 % las primeras abren poco a poco, para que al 60 % ya se vean sin aparecer de golpe. No es un umbral aprobado por Charlie.
- **Calibración:** la candidata inicial (16 activas al 60 % y fundido de 5 %) dejaba **0 ojos** en el arranque normal. Su única observadora en cuadro (rank 2, ID 527) cae en (640, 450), justo detrás de la nave. La curva actual es el ajuste mínimo medido: 28 activas y fundido de 3 %.
- **Precio de la calibración:** en 300 vistas aleatorias sin oclusión, al 60 % hay de media **~2,1 legibles** y **~3,3 dibujados** contando los que aún están abriendo; la peor vista llega a 5. Son cifras medidas con una réplica en Python; el test exige legibles entre 1 y 2,5, dibujados ≤ 3,5 y la peor vista ≤ 5. No se garantiza «1 o 2» en toda cámara.

**Conteos medidos en las vistas reales** a 1280×720 (`matrix.json`):

| O₂ | Arranque normal: ojos / legibles / ocluidos | `--near 3`: ojos / legibles / ocluidos |
|---|---|---|
| 60 % | 2 / 1 / 1 (la nave tapa la rank 2) | 3 / 3 / 1 |
| 50 % | 18 / 15 / 1 | 15 / 12 / 5 |
| 25 % | 83 / 81 / 5 | 53 / 53 / 17 |

## Ojo (huella exacta en píxeles)

- **Forma:** almendra horizontal de semiancho 4,4 px y semialto 2,3 px, que cabe en una caja de **9×5 px** centrada en el píxel de la estrella. Se supermuestrea a 4×4 por píxel.
- **Huella:** un ojo abierto en cielo libre cambia exactamente 37 píxeles (el test exige entre 15 y 45). La estrella original ocupa 1 px.
- **Colores:**
  - esclerótica: el color de la propia estrella × 0,8;
  - iris: ámbar pálido (0,86, 0,62, 0,30) × el brillo de la estrella, con radio de 1,6 px;
  - pupila: casi negra (`0x060308`), con radio de 0,8 px.
- **Apertura:** con el peso, el ojo gana opacidad y abre el párpado desde el 35 % del semialto (una rendija) hasta la almendra completa.
- **Pupila:** el iris y la pupila se desplazan hasta 1,3 px hacia la **nave proyectada con la cámara real** (`camera.view_projection()`, con traslación), no hacia el centro. El desplazamiento se reduce si la nave está a menos de 12 px del ojo.
- **Reserva:** si la nave queda detrás de la cámara (`w ≤ 0`) o su proyección no es finita, `ship_focus_px = None` y las pupilas quedan centradas, mirando al jugador.

## Oclusión y profundidad

- **Profundidades de partida:** las estrellas se escriben a profundidad 1 y el fondo queda en infinito.
- **Sonda:** `SKY_DEPTH_PROBE` es el mayor `f64` menor que 1 (`1.0.next_down()`). `depth_passes(x, y, SKY_DEPTH_PROBE)` solo es verdadero donde la profundidad guardada es ≥ 1, es decir, en el cielo.
- **Centro:** si hay geometría delante del centro, no se dibuja el ojo (`occluded_ids`).
- **Cada píxel del ojo** se pinta solo donde hay cielo. Un ojo junto al borde de un planeta o de la nave queda recortado, nunca encima.
- **Límite:** la geometría escrita exactamente a profundidad 1 (el plano lejano) contaría como cielo.
- **Sin acceso privado ni `unsafe`:** la profundidad solo se consulta con `depth_passes`.

## Validación

Se valida antes de reservar memoria o consultar la profundidad, y cualquier fallo da `Err`; nunca un cuadro negro ni una copia. Se rechaza:
- O₂ NaN, infinito o fuera de [0, 1];
- nave no finita;
- cámara con posición, objetivo o up no finitos, sin dirección de vista (objetivo = posición) o con matrices no finitas;
- `width × height` nulo, desbordado o mayor que `MAX_PIXELS`;
- `pixels.len()` distinto de `width × height`.

**Límite no verificable:** la profundidad es privada de `Framebuffer`. Un cuadro de `Framebuffer::new` la tiene del tamaño de `pixels`; si alguien alterara los campos públicos conservando el producto, esa corrupción privada no se puede detectar desde aquí.

## Preview

```bash
cargo run --release --locked --offline --example watching_stars_preview -- --output-dir <carpeta-nueva>
```

Todo a 1280×720. La carpeta no debe existir y su padre sí; se valida antes de escribir nada.

**Matriz** (`matrix.json`):
- **O₂:** 100, 70, 60, 55, 50, 40, 25, 10 y 0 %.
- **Vistas:**
  - `normal`: el arranque del juego;
  - `near3`: `--near 3`;
  - `sky`: diagnóstico con solo las estrellas, con la cámara del arranque normal. La nave no se dibuja, pero las pupilas miran a su proyección.
- **Escena constante:** en cada vista se conservan la cámara, la geometría, la luz y el tiempo; el World recibe el O₂ de la fila.
- **Tres PNG del MISMO framebuffer original:**
  - `original`;
  - `observers`: los ojos, sin post;
  - `combined`: la percepción aprobada (`apply_lucid_perception`, sin cambiar parámetros) aplicada después de los ojos.
- **Los ojos nunca van después del post** y el negro del 0 % no se levanta.

**Guion** (`sequence.json`, 109 cuadros):
- Arranque normal, con `OxygenTransition` y `advance_perception_frame`.
- Tecla 3 (objetivo 25 %) en el cuadro 6 y tecla 1 (recuperación) en el 60.
- Es un guion de eventos lógicos a 12 fps fijos: no es teclado ni vuelo real.
- El manifest separa objetivo y O₂ mostrado, tiempo del mundo, cámara y nave, IDs activos, proyectados, ocluidos y dibujados, y los pesos parciales.

## Pruebas (`tests/watching_stars.rs`)

El tracer del supervisor es su prefijo intacto. Las pruebas cubren:
- **Catálogo:** bit-exacto contra el fixture inmutable (`tests/fixtures/reference_stars.rs`) en 5 cámaras y 6 tamaños; trasladar la cámara no mueve el cielo; las estrellas detrás de la cámara no tienen píxel.
- **Curva:** cero exacto con O₂ ≥ 65 %; anclas del 60, 50 y 25 %; ranking estable, anidado y monótono; continuidad; recuperación de las mismas identidades; rechazo de O₂ inválido.
- **Ranking:** repartido por la esfera; en 300 vistas aleatorias al 60 % hay pocas observadoras.
- **Render:**
  - huella mayor que 1 px dentro de 9×5;
  - pupila hacia la nave descentrada (informe y píxeles) y reserva sin proyección;
  - oclusión total y parcial y centro ocluido;
  - fuente y profundidad intactas (por sondas);
  - cuadros exactos con O₂ alto en las vistas reales;
  - rechazos;
  - tamaños 1×1, 1×N, N×1 y no cuadrados;
  - informe coherente;
  - determinismo e invariancia a la traslación;
  - estrella sombreada;
  - vistas reales a 60, 50 y 25 %.

## Límites conocidos

- **Percepción combinada:** por debajo del 50 %, el desenfoque y el tinte aprobados convierten los ojos en puntos rosados. Al 25 % ya no se lee la pupila, y con 0 % todo es negro. Se muestra así, honestamente.
- **Escala:** a 1280×720 sin zoom, un ojo de 9×5 se lee como una lenteja clara con un punto oscuro. Su forma de ojo se aprecia mejor con zoom (ver los recortes de la evidencia).
- **Oclusión variable:** el conteo cambia con el encuadre y con lo que tapan la nave y los planetas.
- **No probado:** rendimiento, ventana y teclado.
