# GT3-AUTO: captura automática por contacto en el juego principal

`visual::tentacle_auto_capture` + la ruta `--tentacle-auto-capture` del bucle REAL de `app.rs`. Une la
persecución espacial existente con el gesto aprobado (B1/P1) y el arrastre aprobado (GT1-CARRY) **sin tecla
`T` ni quietud del jugador**. Es opt-in: sin el flag el juego y todas las rutas anteriores no cambian.

```
umbral --creature-solar-reveal --lucid-surface --void-eye --watching-stars \
       --creature-model <obj> --creature-texture <png> \
       --tentacle-auto-capture --tentacle-capture-package <capture_v2> \
       --tentacle-growth-package <growth_package.txt> --tentacle-arm-correspondence <arm_correspondence.txt>
```

Requiere el Sol con ojo y la percepción (`--lucid-perception` o `--watching-stars`: O2 gradual). Las entradas
privadas solo entran por argumentos explícitos; no hay rutas privadas en la fuente.

## Por qué no basta con «pulsar T automáticamente»

La preview C exige 10 cuadros con las matrices de nave dibujada y cuerpo iguales bit a bit antes de capturar, y
cancela si cambian durante B1/P1. El núcleo de B1/P1 (`GraspHandoff::advance`, `GraspClosure::advance`) además
devuelve `Err` si las matrices difieren de las capturadas: es el límite del estudio, no una política de la
preview. Con un jugador que gira o desacelera, la quietud nunca llega. Lo mide la prueba RED
`r02_red_stub`: con un stub que pide `T` en cada cuadro, el contacto real del cuadro 678 no captura.

## Política

1. **Antes del contacto.** Vuelo y controlador reales libres; persecución espacial existente
   (`GraspPreview::frame_on_contact` en su fase de persecución). Con O2 > 15 % la persecución está inactiva y
   no hay captura (prueba `a03`, idéntica a un gemelo del juego sin captura).
2. **Contacto real (`fresh_contact`).** El paso de persecución publicado en ESTE cuadro, sobre el World ya
   movido por la física, cumple todo lo siguiente:
   - `reached` en `Pursuing`;
   - tramo deformado;
   - objetivo SIN recorte (alcance o bola mínima);
   - punta publicada igual, bit a bit, a la posición ACTUAL de la nave.

   No cuentan los estados viejos ni la proximidad a la criatura (prueba `a05`). Después B1 arranca con las
   matrices reales: ancla aprobada, enlace anatómico y capa.
3. **Mismo cuadro.** La nave queda capturada donde ESTÁ (pose del gemelo libre bit a bit) y pierde velocidad e
   inercia (`World::commit_captured_ship_pose`). Desde ahí la entrada de vuelo efectiva es neutra y R/7/8 se
   ignoran.
4. **Cierre B1/P1 (≈8 s, gesto aprobado sin cambios).** Para que las matrices congeladas que exige el núcleo
   sean las REALES, se inmovilizan dos referencias cinemáticas (declaradas):
   - la nave se re-publica cada cuadro en su pose de contacto;
   - el seguimiento corporal no avanza (`World::update_creature_eye_follow_body_held`). Su estado se
     conserva exacto, sin reinicio; el ojo sigue con su mirada.

   Mundo, tiempo, O2, planetas, cámara de persecución, estrellas y post siguen.
5. **Gripped.** Bloqueo, agarre de 0,54 s, arrastre de 3,5 s y retención APROBADOS (mismas funciones de
   GT1-CARRY). El seguimiento corporal se reanuda desde su estado exacto.
6. **Suelta.** O2 > 15 % (regla existente) suelta la nave en su pose, sin inercia. En B1/P1 es la cancelación
   de C; en el arrastre, la suelta aprobada. El empuje mantenido arranca desde 0 con la respuesta de siempre.
   Un error de escena es atómico; el cuadro compartido aborta (nave libre, sin capa). Nunca queda la nave
   bloqueada sin cierre vivo.
7. **Re-armado.** La observación (o el aborto) vuelve a la persecución en reposo el primer cuadro en que la
   capa no se dibuja (ojo oculto, sin salto visible), con el mismo reinicio que X.
8. **X.** Escape de DIAGNÓSTICO heredado (suelta o reinicia). No es una mecánica de juego.

## Bucle

`advance_auto_capture_frame` es el cuadro compartido por la ventana, el replay y las pruebas:

1. preflight numérico (RUNTIME01 aprobado);
2. teclas solares filtradas;
3. física con la entrada efectiva y segundos reales;
4. captura con `dt = min(elapsed, MAX_PHYSICS_DT)`;
5. render con la capa `Body` (mismo compositor solar, estrellas, overlay y post).

En `app.rs` el cuerpo del bucle es el mismo para la ventana (`WindowIo`) y el replay
(`--auto-capture-replay <carpeta vacía>`, `ReplayIo`). El replay usa 1/60 s, un guion reactivo y archivos de
creación exclusiva con el preflight de entradas protegidas.

## Superficie

- **Nuevo:**
  - `src/visual/tentacle_auto_capture.rs`;
  - `tests/tentacle_auto_capture.rs`;
  - `tests/tentacle_auto_capture_app.rs`;
  - esta doc.
- **Aditivo, al final de archivos aprobados** (quitar el bloque restaura el aprobado173 byte a byte; ver
  `ADDITIVE_LEDGER.json`):
  - `GraspPreview::frame_on_contact` y `fresh_contact`;
  - `CaptureCarry::frame_on_contact` y `has_carry_layer`;
  - `World::update_creature_eye_follow_body_held`.
- **`mod.rs`:** una línea.
- **`app.rs` / `main.rs`:**
  - ruta y flag nuevos;
  - `Frontend` (ventana o replay) sustituye las llamadas directas a `minifb` del bucle, sin cambiar su orden;
  - con la captura automática, R/7/8/H/G pasan por el cuadro compartido.

## Medidas y límites

- **Dominio de alcance.** La ley de llegada de la persecución existente deja la punta a v²/(2·48) u detrás de
  una nave a velocidad constante (1,9 u a 15 u/s; 37 u a 60 u/s): con empuje SOSTENIDO no hay alcance
  (`d01`). El alcance ocurre cuando la nave gira en sitio, desacelera, se detiene o se acerca (`d02`). No se
  cambió la persecución; ampliar el dominio (p. ej. contacto por casco) es una decisión de producto.
- **Seguimiento H tras capturar en giro.** El objetivo de H (cámara o nave) queda desfasado del cuerpo y la
  nave sujeta gira con él. En RETENIDA medimos ~6 °/s (`a01`) frente a ~0,7 °/s en la captura manual quieta.
  H OFF lo detiene. Alternativa (checkpoint): mantener el cuerpo inmóvil durante toda la captura.
- **Sin garantía:** ni colisión, ni muerte, audio, regla del 0 %, optimización o publicación.
- **GEO-SAFETY-01 / sp10:** siguen siendo limitaciones históricas de la persecución.

## Evidencia medida (candidato instalado)

- **RED `r02`.** Stub v0 «T automático»: contacto real en el cuadro 678 con la nave girando, sin captura
  (7 fallos conductuales). Nacen en verde `a03` (preservación) y `a05` (predicado).
- **GREEN.** `g01` 9/9 y luego `g06` 10/10. `a10` es una caracterización añadida tras el GREEN.
- **`a10`, mismas fases que la ruta manual aprobada:**
  - B1+P1 dura 478 cuadros y agarre + arrastre 240 en ambas rutas;
  - la forma distal local en el bloqueo difiere como máximo 5,4e-15 OBJ.
- **Lanzador real (`vl01`):**
  - dos replays del bucle principal idénticos;
  - contacto en el cuadro 708 con W + giros + impulso mantenidos;
  - 478 cuadros de cierre con la nave inmóvil;
  - retenida a 93,7 u, con contacto ≤ 9,7e-11 OBJ;
  - primer cuadro libre a 7,4 u/s (un paso desde 0);
  - deriva H en RETENIDA de 6,03 °/s.
- **Fallback (`fb01`).** Los 9 casos son idénticos byte a byte al binario de Root131: rutas sin assets o sin
  la función, rutas aceptadas privadas y la ruta automática al 100 % y al 40 %.
