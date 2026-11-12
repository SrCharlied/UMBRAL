# Estrellas observadoras en el juego (`--watching-stars`, UMB-310)

Integración opt-in de la variante aprobada por Charlie en preview02 («pupila reforzada
progresiva»). El módulo aprobado `src/visual/watching_stars.rs` se compila **sin cambios**
dentro de la biblioteca (`visual::watching_stars`, gracias a `extern crate self as umbral`). La
percepción aprobada (`lucid_perception_variant.rs`, `perception_game.rs`) tampoco cambia.

## Uso

```bash
cargo run --release --locked -- --watching-stars
cargo run --release --locked -- --watching-stars --lucid-surface --void-eye --near 3
cargo run --release --locked -- --creature-model m.obj --creature-texture t.png --creature-solar-reveal --watching-stars
cargo run --release --locked -- --watching-stars --headless --debug-oxygen 0.25 --output renders/ojos.png
```

`--watching-stars` **implica `--lucid-perception`**, así que pasar los dos es redundante y da el
mismo cuadro. Admite las mismas escenas que la percepción: el juego normal (con o sin la criatura
estática y su revelación) o `--creature-solar-reveal`, en ambos con o sin `--lucid-surface` y
`--void-eye`. Sin el flag, todas las rutas, `--lucid-perception` incluida, quedan exactas.

## Composición de cada cuadro

```
escena real (World::render, o SolarGame::render en el Sol con ojo)
  -> ojos aprobados (render_watching_stars: cámara real, nave real, O2 mostrado)
  -> --overlay y paneles, si se piden
  -> percepción aprobada (último paso de color)
```

- El helper de producción es `visual::watching_stars_game::render_game_frame(world, fb, solar,
  watching)`. `app::run` lo usa en el cuadro inicial, en headless y en cada cuadro de la ventana,
  y `P` guarda ese cuadro ya terminado, así que no hay una segunda rama que se salte los ojos. El
  ejemplo y las pruebas usan la misma función.
- **Atómico ante entradas inválidas** (cierre FIX01): con `watching = true`, `render_game_frame`
  comprueba ANTES de dibujar la escena los mismos inputs observables que valida el módulo, en el
  mismo orden y con los mismos mensajes: O₂ mostrado en [0, 1], nave finita, cámara finita con
  dirección de vista y up no nulos, matrices finitas, y `width × height` sin desbordar, > 0, ≤
  `MAX_PIXELS` e igual a `pixels.len()`. Si algo falla, devuelve ese `Err` sin limpiar ni tocar
  el cuadro del llamador. Es un chequeo barato: sin catálogo, ranking ni copia del cuadro. Sin el
  flag no hay preflight y el cuadro es `World::render`/`SolarGame::render` como antes. Límite
  heredado: la profundidad es privada de `Framebuffer`, así que un cuadro con
  `width`/`height`/`pixels` alterados que conserve el producto no es detectable. La rama de
  matrices no finitas se conserva por equivalencia, pero no es alcanzable desde entradas
  públicas finitas.
- `apply_watching_stars(fb, world)` llama al módulo congelado con `world.camera`,
  `world.ship.position` y `world.debug_oxygen01`, y solo reemplaza los píxeles: la profundidad, el
  cielo y su semántica de profundidad siguen siendo los del módulo.
- `GameFrame.stats` son las estadísticas del render 3D de la escena; los ojos (un paso de color)
  no aportan nada a ellas. `GameFrame.watching_stars` es el informe del módulo: IDs, ojos, foco y
  ocluidas.
- Los ojos van antes del overlay: la banda semitransparente oscurece los ojos que tiene debajo,
  pero ningún glifo cambia. Con O₂ 0 la percepción deja todo en negro, overlay incluido.

## Estado y controles

> **Actualización (integración con el gameplay de Persona 2):** sin `--debug-oxygen`, el objetivo
> de O₂ es el **oxígeno real** de `GameplayState`, que se consume solo. Por eso los ojos aparecen
> en partida normal cuando el O₂ mostrado baja de 65 %: hacia los 147 s con el tanque de 420 s, o
> antes con `--oxygen-seconds S` o la tecla `L` (ver README, «Oxígeno para demostraciones»).
> `tests/oxygen_pace.rs` lo verifica de extremo a extremo sin teclas. Lo que sigue describe el
> modo manual (`1`–`4`, `M`/`N`), que sigue disponible como override visual.

- Se reutilizan `OxygenTransition` y `app::advance_perception_frame`: `1`–`4` y `M`/`N` cambian
  el objetivo, y el O₂ mostrado lo sigue a 0,25 por segundo real, simétrico al bajar y al
  recuperar, llega exactamente (también a 0) y nada lo consume solo. El O₂ inicial es exactamente
  `--debug-oxygen`, sin presimulación.
- Los ojos leen el O₂ **mostrado** (`world.debug_oxygen01`), el mismo que ven el Sol, Antena, la
  revelación y la percepción. La población es función de ese valor: con el mismo O₂, mismas
  observadoras al bajar y al recuperar.
- `F5` congela las órbitas, no la transición ni la aparición. `R` conserva objetivo y O₂ mostrado.
- Las pupilas miran siempre a la nave proyectada con la cámara real del cuadro, también en el Sol
  con ojo; `G` (objetivo de la mirada de la criatura) no las afecta.
- No hay HUD, teclas, paneles, ataques ni consumo automático nuevos.

## Validación (antes de cualquier IO)

- **Línea de comandos:** `validate_watching_stars_flags` rechaza `--legacy` y, fuera del Sol con
  ojo, `--creature-eye-compare`, `--creature-distant-turn` y `--creature-horizontal-follow`, con un
  error que nombra el flag. Corre antes de expandir los flags implícitos del Sol, como los demás
  validadores.
- **API pública:** `app::run_game_with_watching_stars(options, PerceptionRoute)` aplica los mismos
  guards que `run_game_with_lucid_perception`: tamaño (`checked_mul` y máximo de píxeles), tiempo
  finito ≥ 0, O₂ en [0, 1], los guards del juego normal o del Sol con ojo, `--output` `.png`
  también sin criatura, y alias de `--output` con las entradas de la criatura. `GameOptions` y
  `PerceptionRoute` no cambian.

## Pruebas y evidencia

- `tests/watching_stars_game.rs`: el tracer del padre (CLI real contra el módulo congelado y la
  percepción aprobada) y, a continuación, las pruebas de la integración:
  - el validador;
  - el helper de producción frente a preview02 en el juego normal y en `--near 3`;
  - la CLI: guards, dependencia implícita, Sol, superficie lúcida y Void;
  - el orden del overlay;
  - la transición de O₂ con F5, R, el 0 % y la recuperación;
  - el replay controlador → mundo → cámara → ojos;
  - que las rutas viejas no reciben ojos.
- `examples/watching_stars_game_sequence.rs`: guion determinista con los helpers reales (640x360
  por defecto, `--width`/`--height`, carpeta nueva): caída, cambio de objetivo a mitad, `N`, `F5`,
  `R`, 0 % y recuperación, con un PNG por cuadro y `manifest.json` (O₂ objetivo, mostrado y del
  mundo, ojos, cámara, foco y reloj). Es un guion lógico: no es teclado físico ni ventana, y no
  mide FPS.

## Límites

- No se probó la ventana ni el teclado físico. El bucle de ventana usa el mismo helper que
  headless, pero su ejecución no está cubierta por pruebas automáticas.
- Coste por cuadro sin medir: el módulo recalcula catálogo, ranking y ojos de las 700 estrellas en
  cada cuadro (no optimizado, por encargo).
- Al 25 % de O₂, con la percepción, los ojos se leen sutiles; Charlie lo aceptó.
