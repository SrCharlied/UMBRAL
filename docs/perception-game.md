# Percepción alterada en el juego (`--lucid-perception`, UMB-310)

**Estado:** integración opt-in de la variante 02 aprobada por Charlie («Simon, que sea progresivo»). Sin el flag, el juego es idéntico.

- La apariencia está **congelada**: `src/visual/lucid_perception_variant.rs` se usa sin un solo byte cambiado. Su contrato visual está en [`lucid-perception-variant.md`](lucid-perception-variant.md).
- Lo nuevo es solo la integración: flag, ruta en el bucle común, foco, transición progresiva del O₂ manual, validación y pruebas.
- **No incluye** combate, ataque, daño, duración, cooldown, consumo automático de O₂, estrellas observadoras, HUD ni audio. El `attack_blackout` del módulo queda siempre en `false` en el juego.

## Uso

```bash
cargo run --release --locked -- --lucid-perception --near 3
cargo run --release --locked -- --lucid-perception --lucid-surface --void-eye --near 3
cargo run --release --locked -- --creature-model m.obj --creature-texture t.png --creature-solar-reveal --lucid-perception [--lucid-surface] [--void-eye]
```

**Rutas:** el juego normal (también con la criatura estática y su revelación) o el Sol con ojo, en ambos con o sin `--lucid-surface` y `--void-eye`.

**Rechazos** (error claro y exit 1 antes de leer archivos, incluso la nave; nunca se apaga un flag en silencio):
- `--legacy`;
- fuera del Sol con ojo, `--creature-eye-compare`, `--creature-distant-turn` y `--creature-horizontal-follow`. Se valida antes de expandir los flags implícitos del Sol;
- tamaño nulo o mayor que `MAX_PIXELS` (incluido el desbordamiento), tiempo u O₂ inválidos;
- `--output` que no sea `.png` (**también sin criatura**) o que sea un alias de una entrada.

**Biblioteca:** `app::run_game_with_lucid_perception(options, PerceptionRoute { lucid, void, solar })` valida lo mismo antes de cualquier IO. `GameOptions` y las entradas anteriores no cambian.

## Cuadro

El mundo se dibuja **como siempre**, con el O₂ y el reloj vivos. No se fija el O₂ del mundo a 1 como en el preview: el Sol, Antena y la revelación ven el mismo O₂.

Después, el **último paso de color** es:

```text
lucid_perception_variant(cuadro, world.debug_oxygen01, world.time, ship_focus(world).uv, false)
```

- Se aplica tras el overlay (`F1`/`--overlay`) y los paneles; con O₂ 0 no queda texto encendido.
- Usa la misma función en el cuadro headless, en cada cuadro de la ventana y en `P`.
- Solo reemplaza `pixels`: no toca profundidad, cámara, transformaciones ni geometría.
- No añade overlays.

**Foco (`ship_focus`):** `world.ship.position` proyectada con `world.camera.view_projection()`, la cámara REAL del cuadro, no el centro fijo.
- En el arranque `--near 3` a 1280×720 queda en (640, 455) px. La cámara de persecución mantiene la nave casi siempre en ese punto (también en los arranques `--near 1` y del Sol con ojo registrados); se desplaza con el retraso de la cámara al girar y con el FOV de la velocidad.
- Si la nave está delante pero fuera de pantalla, el foco se acota al borde más cercano.
- Si queda detrás de la cámara (`w <= 0`) o la proyección no es finita, el foco es el centro (`FALLBACK_FOCUS_UV`) con `projected = false`, y el log del arranque lo dice.
- Con la cámara de persecución del juego (también en las vistas del Sol con ojo) la nave siempre queda delante y en pantalla.

## Progresividad del O₂ manual

`OxygenTransition` separa el **objetivo** (teclas) del **O₂ mostrado**:

| Regla | Comportamiento |
|---|---|
| Teclas | `1`–`4` (100/50/25/5 %) y `M`/`N` (±5 %, `step_debug_oxygen`) cambian solo el objetivo, con sus significados y su orden de siempre (preset y después M/N). |
| Transición | El O₂ mostrado avanza en línea recta a `OXYGEN_TRANSITION_RATE` = **0.25 O₂ por segundo real**, al bajar y al recuperar (100 % → 0 en 4 s). Es un parámetro técnico provisional de respuesta, no un ajuste artístico. |
| Llegada | Exacta: si el resto cabe en el paso, toma el objetivo. No se pasa ni se acerca para siempre; llega a **0 exacto** en tiempo finito, y el negro total solo ocurre con el O₂ mostrado = 0. |
| Reloj | Segundos REALES del bucle (`Instant`, monotónico), enteros: no el `dt` limitado de la física (`MAX_PHYSICS_DT`) ni el reloj orbital. `F5` no la congela. |
| Durante la transición | Otro preset o `M`/`N` cambian el objetivo y el mostrado sigue desde donde está (reversión sin salto); sin más teclas la transición continúa sola. |
| `R` | Conserva el O₂ mostrado y el objetivo. |
| Arranque | Headless y ventana arrancan EXACTAMENTE en `--debug-oxygen` (objetivo = mostrado), sin presimular. |
| Consumo | Nada consume O₂ solo. |
| Errores | `dt` negativo o no finito, o preset fuera de `[0, 1]`: `Err` sin tocar nada. `dt` 0 no cambia nada. |

Cada cuadro de la ventana llama a `app::advance_perception_frame(world, oxygen, keys, input, elapsed)`:
1. teclas → objetivo;
2. el mostrado avanza con `elapsed`;
3. `world.debug_oxygen01 = mostrado`;
4. `advance_frame` de siempre.

Es atómico ante entradas inválidas. Sin el flag, el bucle salta el O₂ como siempre.

**Título:** `PERCEPCION | O2 x% -> objetivo y% (1-4, M/N)`, junto al estado del Sol con ojo si aplica.

## Mecanismo de compilación

El módulo congelado nombra `umbral::renderer::framebuffer`, porque nació para compilarse por `#[path]` en pruebas y ejemplos. Para usarlo desde la biblioteca sin reescribirlo, `src/lib.rs` declara `extern crate self as umbral;` y `visual/mod.rs` lo exporta (`visual::lucid_perception_variant`). Sus pruebas y su ejemplo siguen compilándolo por `#[path]` sin cambios.

## Evidencia: `examples/perception_game_sequence.rs`

```bash
cargo run --release --locked --offline --example perception_game_sequence -- --output-dir <carpeta-nueva> [--width N --height N]
```

**Escena:** la de `--lucid-perception --lucid-surface --void-eye --near 3 --time 1.3`, a 640×360 por defecto, con los helpers reales: `OxygenTransition`, `advance_perception_frame`, `apply_lucid_perception` y `ship_focus`.

**Guion** de 163 cuadros a 12 fps de segundos reales fijos (eventos lógicos, no teclado físico):

| Segundo | Evento |
|---|---|
| 0.5 | tecla 4 |
| 3.5 | tecla 1 a mitad de la caída (reversión) |
| 5.0 | tecla 4 |
| 5–7 | `F5` |
| 8.0 | `N` → 0 % (blackout) |
| 9.0 | tecla 1 (recuperación) |

La nave no recibe mando; Vigilia se aleja porque sigue su órbita.

**`manifest.json`:** por cuadro registra los segundos reales, el tiempo del mundo, `F5`, el evento, el objetivo, el mostrado, `world.debug_oxygen01`, el foco, el título, la fracción negra y la luma.

## Pruebas

**`tests/perception_game.rs`** (el tracer del supervisor es su prefijo intacto):
- CLI real frente al módulo aprobado al 100/50/25/10/5/1/0 % a 320×180 y 1280×720: identidad con O₂ ≥ 50 % y negro total solo con 0;
- rutas normales con `--lucid-surface`/`--void-eye` (Antena y Vigilia en cuadro);
- las 4 rutas del Sol con ojo (criatura de fixture);
- foco real y nave descentrada;
- reserva sin proyección;
- postproceso tras `--overlay`;
- sin flag idéntico;
- validador y rechazos de la CLI y de la biblioteca antes de IO.

**`tests/perception_progress.rs`:**
- arranque exacto;
- caída y recuperación lineal;
- 60 fps sin saltos ni overshoot y con llegada exacta;
- cero exacto en tiempo finito;
- `dt` real largo;
- `dt` 0 y valores no finitos atómicos;
- presets y `M`/`N` a mitad;
- sin consumo;
- cuadro con el mundo real: O₂ gradual en el mundo, `F5`, atomicidad y `R`.

## Límites conocidos

- **Costo:** con O₂ < 50 %, el desenfoque y el túnel recorren el cuadro completo en CPU (a 720p cuesta más por cuadro). No se ha medido ni optimizado; la optimización quedó diferida.
- **Overlay:** `F1` también se tiñe y desenfoca bajo 50 % (es el último paso). El estado legible está en el título.
- **Teclado:** el bucle de ventana y sus teclas no tienen prueba automática ni prueba física registrada; la lógica que usan (`advance_perception_frame`) sí está probada.
- **Combinaciones visuales (observadas, no ajustadas):**
  - el tinte vuelve moradas las grietas rojo-ámbar de Antena;
  - el Sol se lee rosa al 25 %;
  - en el Sol con ojo, la criatura (también morada) queda dentro del túnel al 10 % pero se funde con el tinte y, al 5 %, el túnel recorta sus tentáculos.
