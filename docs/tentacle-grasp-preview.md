# Preview C del agarre experimental: persecución → llegada → B1 → P1

**Estado:** preview manual AISLADA para que Charlie vea el candidato P1 tal cual. **No** está conectada a
`app`/`main`, no es captura jugable ni aceptación artística. Contrato: `PREVIEW_CONTRACT.json` (carpeta de la
entrega).

- Módulo: `src/visual/tentacle_grasp_preview.rs` (estado, carga, protección de entradas).
- Ejemplo: `examples/tentacle_grasp_preview.rs` (ventana y secuencia headless).
- Pruebas: `tests/tentacle_grasp_preview.rs` (assets privados opt-in, `--ignored`).

## Ruta

Es la escena del lanzador espacial anterior, sin cambios:
- World con la nave real, superficie lúcida, ojo del vacío, Sol con ojo y percepción con estrellas.
- Cada cuadro se dibuja con `render_game_frame_with`, y después se aplica `apply_lucid_perception`.
- La capa del tentáculo entra como `SolarBodyOverride` en esa MISMA escena: no hay Blender ni secuencia
  pregrabada.

Orden de cada cuadro (ventana y headless):
1. Se leen las teclas.
2. `advance_perception_frame`: el World se actualiza SIEMPRE primero, también durante B1/P1.
3. `GraspPreview::frame(&world, dt, {T, X})`: la preview nunca escribe el World.
4. Render, percepción y título.

## Propiedad (sin `unsafe` ni auto‑referencias)

`ClosureClip<'p>` toma prestado un `GrowthPackage`, y la persecución necesita `&mut` en cada cuadro.
`GrowthPackage` no es `Clone` y el núcleo no se toca. Por eso `load_preview_inputs`:
- lee las entradas UNA vez (manifest, OBJ verificados por tamaño y FNV, growth y correspondencia);
- con esos mismos bytes construye `SolarCaptureDebug::from_parts` (que pasa a la persecución) y
  `GrowthPackage::from_parts` (inmutable, vive en `main` y presta al clip).

`GraspPreview::new` y cada reinicio exigen que el rig enlazado de la persecución sea, bit a bit, el del clip.
B1 se crea con `clip.adapter()` y P1 comprueba esa identidad.

## Controles

- **De siempre:** W/S empuje; A/D o flechas izquierda/derecha guiñada; flechas arriba/abajo cabeceo; Q/E
  alabeo; Shift impulso.
- **Oxígeno:** 1‑4 fijan el objetivo de O2 (100/50/25/5 %); M/N lo suben o bajan.
- **Ruta solar:** 7 vista FRONTAL, 8 OBLICUA, R vuelve a FRONTAL, H seguimiento, G mirada.
- **Varias:** F1 overlay, F3‑F6 interruptores, P captura y Esc salir.
- **Nuevas:** G y R ya estaban ocupadas, así que se usan teclas libres comprobadas.
  - **T** arma o desarma la solicitud de agarre; puede pulsarse antes de la llegada.
  - **X** reinicia el experimento: `bind` con el World actual, la punta vuelve al reposo y persigue otra vez.
    No toca nave, cámara ni O2.

La solicitud se cumple en el primer cuadro en que se dan las dos condiciones:
- Ese mismo cuadro la persecución publicó un paso con modo `Pursuing`, brazo y `reached`.
- Las matrices de nave dibujada y cuerpo llevan `QUIET_FRAMES` = 10 cuadros seguidos iguales bit a bit.

Entonces se toma `ReachedPose::capture` de ESE paso (llegada actual, no vieja) y B1 arranca con las matrices
actuales.

## Política de estudio (frescura)

En cada cuadro de B1/P1, ANTES de `advance`:
- Si las matrices de nave o cuerpo difieren en un solo bit de las capturadas, el episodio pasa a
  **Observación CANCELADA**.
- Lo mismo si el O2 mostrado supera el 15 %.

La observación cancelada funciona así:
- Queda dibujada la última malla publicada, y el título lo declara.
- No se vuelve a llamar a `advance`, no se publica contacto vigente ni se salta automáticamente a otra forma.
- La nave, la cámara y el input siguen libres; X reinicia.

Otros casos:
- **Solo cámara:** cambiar únicamente la cámara (de observación) no cancela.
- **Error del núcleo:** pasa a **Observación con ERROR**, con la última malla y sin capa parcial (la capa solo
  se escribe con buffers ya validados).
- **`dt` no finito o fuera de `[0, MAX_PURSUIT_DT]`:** `Err` y no cambia nada.
- **Sin solicitud, u O2 alto:** el cuadro y las estadísticas son los de la ruta espacial anterior.

### Hallazgo del World (diagnósticos d01, d03 y d04)

**Tras volar con empuje:** la nave y el cuerpo quedan quietos bit a bit en ≤ 1280 cuadros (~21 s a 60 fps).

**Tras ciertos giros:** no se aquietan solos.
- La cámara entra en un ciclo de 2 posiciones (4e‑15).
- El seguimiento horizontal del cuerpo hacia la cámara arrastra al cuerpo a un ciclo de 2 matrices
  (Δ = 8,9e‑16, deriva neta 0) que se mantuvo los ≥ 6000 cuadros medidos.
- Con la política estricta la solicitud espera indefinidamente: el título lo dice y la ayuda indica **R** (vista
  FRONTAL). H también lo resuelve. Con cualquiera de las dos, en ~880 cuadros (~15 s) vuelve la quietud exacta.
- No se introdujo tolerancia: es el límite numérico de B1/P1 (matrices fijas bit a bit).

## Salidas

### Headless: `--headless-sequence <carpeta NUEVA>`

**Guion determinista** a `dt` 1/60, con las teclas en el guion:
1. Pulsa 4 en el cuadro 0 y T en el 30.
2. Primer episodio completo y 60 cuadros de hold.
3. X, T y segundo episodio.
4. En la preparación P1 (cuadro autorado ≥ 60), empuje y guiñada: observación cancelada.
5. X y persecución de la nave movida.

**Protección de la salida:**
- La carpeta se crea con `create_dir` exclusivo y cada archivo con `create_new`.
- Cada PNG pasa por `validate_output_path`.
- Las entradas efectivas quedan abiertas con FILE_SHARE_READ, y su huella FNV se registra antes y después.

**Contenido de la carpeta:**
- 13 instantes en tres vistas cada uno:
  - `game`: el cuadro completo del juego a 1280×720.
  - `overview_DIAGNOSTICO` y `cercana_DIAGNOSTICO`: cámaras de diagnóstico sobre el MISMO World, sin
    percepción, con la cámara guardada y restaurada. Llevan marcas cian (nave), magenta (centroide grip de la
    malla) y amarilla (ancla transportada).
- `metrics.json` estricto:
  - una fila por cuadro;
  - eventos, capturas, recuentos por fase reconciliados, y error de contacto medido sobre la MALLA publicada;
  - valores no finitos o no proyectables como `null`.

### Ventana

Opciones: `--debug-oxygen X`, `--frames N` y `--capture-dir DIR` (P guarda con `create_new`). La ayuda se
imprime al abrir y el estado va en el título; no hay HUD sobre la imagen.

## Candidato tal cual (no retocado)

- **Corte fijado:** las caras D finas que tocan el corte tienen hasta ×2,9 de área respecto al arte; el seam no
  tiene hueco.
- **Carrier anclado:** la punta se mueve ~0,78 OBJ (~56 u) en 7 s.
- **Contacto:** es posicional, no de orientación.
- **Muestreo:** cuantizado a 24 fps.
- **Colisión:** sin certificado global.
- **B1:** desde la llegada de la persecución, el grip recorre en 1 s una distancia grande en pantalla.

## Límites

- Las 8 cuadros de ventana del verificador solo prueban el arranque: el teclado físico y el juicio visual son
  de Charlie.
- El headless ejercita la solicitud por guion, no por teclado.
- La quietud bit a bit puede tardar o, tras ciertos giros, requerir R o H (ver el hallazgo).
- No hay medición de rendimiento ni benchmark.
