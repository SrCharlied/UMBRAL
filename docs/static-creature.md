# Criatura estática en el juego (ruta debug, opt-in)

**Qué es:** una entidad opcional del `World` que dibuja el modelo texturizado de la criatura **dentro del juego real**. Usa el mismo framebuffer, el mismo z-buffer y la misma cámara de persecución de la nave que el resto de la escena, y se controla con los controles normales de la nave. Usa `draw_model_with` y `TexturedShader` (ver [`textured-preview.md`](textured-preview.md)).

**Qué no es:**
- **No usa el render del visor** ([`creature-viewer.md`](creature-viewer.md)): `render_view` limpia el cuadro y crea su propia cámara.
- **No es una ubicación final del nivel.**
- **No incluye** animación, LOD, mipmaps, física ni colisiones. La **revelación por oxígeno** es otra opción aparte (`--creature-reveal`, ver [más abajo](#revelación-por-oxígeno-creature-reveal)); sin ella la criatura siempre es visible. La **mirada del ojo hacia la nave** también es opcional y solo existe con la revelación por etapas (`--creature-eye-follow`, ver [más abajo](#mirada-local-creature-eye-follow)).
- **No sustituye** al Sol ni a ningún planeta, salvo con `--creature-solar-reveal`, que funde la representación del Sol con la criatura (ver [más abajo](#sol-con-ojo-en-el-juego-creature-solar-reveal)).

**Sin flags,** el juego es idéntico al anterior: misma imagen, mismos contadores, mismo comportamiento y ninguna lectura de archivos externos.

## Uso

```bash
cargo run --release -- --creature-model ruta/al/modelo.obj --creature-texture ruta/a/textura.png [--near-creature]
cargo run --release -- --headless --output salida.png --creature-model ... --creature-texture ... [--near-creature]
```

| Flag | Contrato |
|---|---|
| `--creature-model OBJ` y `--creature-texture PNG` | Siempre juntos. Activan la criatura, que se carga **una sola vez** al arrancar; el mundo toma posesión del modelo y de la textura. Mismos formatos que el preview: OBJ con su MTL al lado (no se lee `map_Kd`) y PNG RGB de 8 bits |
| `--near-creature` | Requiere la criatura. Arranca la nave frente a ella, mirándola. Con esta opción, `R` vuelve **a ese arranque**; sin ella, `R` restablece la nave como siempre |

**Rechazos antes de leer archivos o abrir la ventana** (código 1 y mensaje `Error: …`):
- Un par incompleto o un flag sin ruta.
- `--near-creature` sin la criatura.
- `--near N` junto con `--near-creature`: son dos arranques distintos.
- Cualquier flag de la criatura con `--legacy`.
- Con criatura, un `--output` que no es `.png` o que resuelve a la misma ruta que el modelo o la textura (`validate_output_path`).

**Si falla la carga,** el error nombra la ruta. Las rutas solo llegan por la CLI: no hay rutas fijas y el asset no está en Git.

**Controles:** los de siempre de la nave. No se añadieron controles orbitales del visor.

## Ubicación de demostración

| Parámetro | Valor |
|---|---|
| Centro de la esfera envolvente | `(0, 0, −900)` (`DEMO_CENTER`) |
| Radio en mundo | 80 (`DEMO_WORLD_RADIUS`) |
| Giro | 0: el frente original del OBJ (+Z) mira al Sol |

- **Separación:** la mayor extensión orbital es 540 (`max(orbit.radius + body_radius)`; Vigilia: 520 + 20). La esfera de la criatura empieza en 900 − 80 = 820, así que ningún cuerpo la toca en ningún instante. Lo comprueba una prueba con `SolarSystem::demo()`, sin suponer nada por el nombre del asset.
- **Normalización:** el OBJ no está centrado en su origen. La escala es uniforme, `radio_mundo / bounds_radius`, y la traslación compensa el centro envolvente girado y escalado, de modo que la esfera queda exactamente en el centro elegido.
- **Arranque `--near-creature`:** la nave queda en el lado del frente, a 2.5 radios del centro (`SPAWN_DISTANCE_RADII`), mirando al centro. Se calibró con la cámara de persecución real (FOV 60°, 22 detrás y 6 arriba): el ojo queda por encima de la nave y la esfera entera cabe en el FOV vertical (lo comprueba una prueba). A 3.0 radios la criatura se ve más pequeña; es una decisión de legibilidad debug, no artística.
- **Desde el arranque normal,** con la criatura cargada y sin `--near-creature`, la criatura queda **justo detrás del Sol**. Está dentro del frustum, así que paga vértices y rasterizado, pero el z-test la oculta por completo: la imagen es idéntica a la de sin criatura.

## Contratos (`src/visual/static_creature.rs`)

| Pieza | Contrato |
|---|---|
| `CreaturePlacement::new(center, world_radius, yaw)` | Componentes del centro en `[−1e6, 1e6]`, radio en `[1e-3, 1e5]` y yaw finito en `[−2π, 2π]`. Si no, `Err` |
| `transform_for(model_center, model_radius)` | Los bounds del modelo siguen el contrato puro `CreatureBounds` del visor (solo esa validación; no usa su cámara ni su render). Devuelve `Err` si el transform no es finito |
| `StaticCreature::new` / `load` | Toman posesión de `Model` y `Texture`. El transform se calcula **una vez** y no cambia con el tiempo ni con `World::update` |
| `World::set_creature` / `clear_creature` / `creature` | `World::new` no tiene criatura. `set_creature` comprueba antes que la luz real del mundo sirve al shader |
| `World::place_near_creature` | Coloca la nave en `spawn_pose()`. No cambia el tiempo ni los cuerpos y es repetible exactamente; sin criatura devuelve `Err` |

### Render en `World::render`

- **Orden:** cuerpos, criatura y nave, sobre el mismo framebuffer. La entidad **no limpia** color ni profundidad: compite por z-test con planetas y nave.
- **Luz real:** la del Sol (`SolarSystem::light_position()`, el origen), en modo `Lit` con ambiente 0.25. Sin luz de cabeza, sin tinte (el `Kd` se ignora) y sin el patrón de `LitPlanet`.
- **Doble cara siempre,** aunque el descarte global de caras traseras (F6) esté activo: la malla de la criatura no es cerrada (25 aristas de borde según la inspección en Blender), y descartar caras traseras abriría huecos.
- **Descarte por frustum antes de transformar,** con la esfera envolvente en mundo; respeta `--no-culling` y F3.
  - Descartada: cuenta 1 objeto enviado y descartado, y sus caras como enviadas y descartadas (como los cuerpos). Cero vértices y cero rasterizado.
  - Con `--no-culling` paga la etapa de vértices aunque los planos de recorte descarten todos sus triángulos.
- **Contadores:** se suman a los del mundo sin contar dos veces.

## Pruebas (`tests/static_creature.rs`, 13)

Todos los fixtures son sintéticos: un cubo descentrado con UV y normales, y una textura RGB de 2×2. No necesitan el asset ni abren ventanas.

- **Colocación:**
  - El modelo descentrado cae exactamente en la esfera elegida con 3 giros, con escala uniforme y el frente en `front()`.
  - Se rechazan valores inválidos, y los extremos válidos dan resultados finitos.
  - La demo queda fuera de todas las órbitas, con un margen de más de 100, y dentro del plano lejano.
- **Carga:** la criatura toma posesión de modelo y textura y el transform sale de los bounds. `load` nombra la ruta que falla y rechaza RGBA.
- **Flags:** `CreatureOptions::from_flags` es puro y no hace IO.
- **Binario** (siempre `--headless`):
  - Las combinaciones inválidas terminan con código 1 sin crear PNG; las rutas inexistentes demuestran que el error es de validación.
  - Una captura igual a la textura se rechaza y la textura queda intacta.
  - El cableado completo dibuja 6 objetos.
- **`World`:**
  - Sin criatura el mundo tiene 5 objetos, y quitar la criatura devuelve exactamente los mismos píxeles y contadores.
  - Visible: la imagen es idéntica a dibujar la criatura sobre el cuadro sin ella con el z-buffer intacto, la cámara de la nave, la luz del Sol y doble cara (0 caras traseras con F6 activo). Los contadores aditivos son exactos.
  - Fuera de vista: con descarte suma `[1, 1, 0, 12, 12, 0, 0, 0]`; con `--no-culling`, `[1, 0, 8, 12, 12, 0, 0, 0]`. En los dos casos la imagen y los fragmentos son idénticos.
  - Profundidad: detrás de un planeta se rasteriza pero no cambia ningún píxel; delante lo tapa, y solo cambian píxeles dentro de su círculo proyectado. La nave tapa a la criatura; sin la nave, la criatura ocupa esos píxeles.
  - Estática: el transform y la malla no cambian tras 120 `update` con controles, mientras la cámara sí se mueve.
  - `--near-creature`: la nave queda del lado del frente, a la distancia exacta, mirando al centro y con la esfera dentro del FOV. Repetirlo da el mismo arranque bit a bit, sin cambiar el tiempo ni los cuerpos.

## Medición

Tiempo real de `World::render` completo (limpiar, estrellas, cuerpos, criatura y nave) a 1280×720, con la criatura real (31 096 triángulos) y la textura candidata de 1024². Build release, en serie y sin otros trabajos; 2 calentamientos y 11 mediciones. Se midió con un *harness* desechable fuera del repositorio:

| Escena (controles reales de la nave) | Con criatura | Sin criatura |
|---|---|---|
| Arranque `--near-creature` (2.5 radios) | 59.8 ms | 5.1 ms |
| Cerca (1.47 radios, tras 2 s de empuje) | 82.7 ms | 4.9 ms |
| Fuera del frustum (tras girar) | 4.4 ms | 5.0 ms |
| Fuera del frustum, `--no-culling` | 9.5 ms | 6.4 ms |
| Arranque normal (oculta detrás del Sol) | 22.7 ms | 8.5 ms |

- **Con la criatura en vista, el cuadro del juego se vuelve mucho más lento.** Es un dato medido, no un perfil, y no atribuye el coste a una etapa concreta. Con estos números no se pueden prometer 60 FPS.
- **Fuera del frustum,** la diferencia está dentro del ruido. Sin descarte se pagan los vértices.
- **El FPS del overlay (F1) no sirve como benchmark para cuadros lentos:** `app.rs` limita `dt` a 0.1 s y lo suaviza. Esta tarea no lo corrige.

## Revelación por oxígeno (`--creature-reveal`)

Primera candidata, opt-in. Con poco oxígeno, la criatura aparece poco a poco como una presencia translúcida sobre la escena real. Si el oxígeno se recupera, vuelve a ocultarse gradualmente. Su aprobación artística está pendiente.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --near-creature --creature-reveal [--debug-oxygen 0.25]
```

| Flag | Contrato |
|---|---|
| `--creature-reveal` | Requiere `--creature-model` y `--creature-texture`. **No** implica `--near-creature` ni mueve la nave. Con `--legacy` se rechaza |
| `--debug-oxygen X` | Oxígeno de depuración inicial, finito en `[0, 1]` (por defecto 1), fijado antes del primer cuadro. También vale sin criatura, para comparar al mismo O₂; el cuadro del juego normal no cambia. Con `--legacy` se rechaza. Las teclas `1`–`4` siguen igual: 1.0, 0.5, 0.25 y 0.05 |

Los dos se validan antes de leer archivos. Sin estos flags, el juego y la criatura estática no cambian.

Para recorrer el medidor manualmente, `M` aumenta el oxígeno y `N` lo reduce en **5 puntos
porcentuales por pulsación**, con límites de 0 % y 100 %. Mantener la tecla no repite pasos;
ambas teclas juntas se cancelan. El medidor se ajusta a porcentajes enteros al dar un paso,
sin deriva acumulativa en los umbrales. Los presets `1`–`4` se conservan y se aplican antes
de `M`/`N` si coinciden en un cuadro. El porcentaje actual aparece en el overlay (`F1`).

### Estado (`src/visual/creature_reveal.rs`)

- **Objetivo:** `reveal_target(O₂)` es un `smoothstep` continuo entre el umbral de aviso de UMB-310 (`start_oxygen` 0.50, oculta) y el crítico (`full_oxygen` 0.10, entera).

  | Tecla | O₂ | Objetivo |
  |---|---|---|
  | `1` | 1.00 | 0 |
  | `2` | 0.50 | 0 |
  | `3` | 0.25 | 0.684 |
  | `4` | 0.05 | 1 |

- **Transición:** `step_reveal` persigue el objetivo con tiempo **real**. Llega al 95 % en `rise_seconds` (2 s) al aparecer y en `fall_seconds` (0.7 s) al ocultarse. Los dos tiempos son ajustables en `RevealSettings::new` y se validan en `[0.01, 60]` s.
  - Cuando faltan menos de `REVEAL_SNAP_EPSILON` (0.001) para el objetivo, el valor se fija exactamente en él. Por eso 0 y 1 se alcanzan de verdad: ir de 0 a 1 tarda ~4.6 s y de 1 a 0, ~1.6 s.
  - Trocear el tiempo en cuadros no cambia el resultado (salvo ese ajuste final).
- **Tiempo real:** `World::update_creature_reveal(segundos)` es un paso visual separado de `World::update`. `app.rs` le pasa el tiempo real entre cuadros **sin** el límite de 0.1 s del `dt` físico.
  - El efecto no suma al tiempo orbital, `F5` no lo congela y la física y la cámara no cambian.
  - Un cuadro muy largo, por ejemplo al arrastrar la ventana, lo lleva casi directo al objetivo.
- **Estado puro:** solo guarda los ajustes y la visibilidad mostrada. No depende del tiempo absoluto, y la malla, el transform y la textura nunca cambian.
- **Saltos al objetivo sin transición:** al activar el modo, con `set_creature`, `R` (`reset_ship` o `place_near_creature`), `place_near` y `snap_creature_reveal`. `R` conserva el oxígeno. En headless se arranca ya en el objetivo, así que las capturas son deterministas.
  - `clear_creature` conserva el modo, y `disable_creature_reveal` vuelve a la criatura siempre visible.
- **Política para valores no finitos:**
  - Un O₂ no finito deja la criatura oculta.
  - Un objetivo no finito se trata como 0.
  - Una visibilidad no finita salta al objetivo.
  - Un tiempo NaN, negativo o cero no avanza; un tiempo infinito llega al objetivo.
  - Todo resultado es finito y está en `[0, 1]`.

### Render

| Visibilidad | Qué se dibuja |
|---|---|
| 0 | **Nada**: ni se rasteriza, ni se escribe profundidad, ni se cuenta. Píxeles y `RenderStats` son idénticos a no tener criatura |
| 1 | La ruta estática de siempre, en el mismo orden (antes de la nave). Píxeles y contadores son idénticos a la variante estática |
| Entre 0 y 1 | `RevealCompositor`, un **fantasma de una sola capa** dibujado **después** de cuerpos, estrellas y nave |

Pasos del compositor en la visibilidad parcial:
1. Copia los píxeles del cuadro real.
2. Dibuja la criatura opaca e iluminada, como la estática, con el z-buffer real.
3. Anota en una máscara los píxeles cuyos fragmentos aceptaron el z-test. En `triangle_shaded`, sombrear implica escribir, así que cada píxel anotado termina con la superficie más cercana.
4. Mezcla **una sola vez por píxel**, en luz lineal: `fondo + (criatura − fondo) · visibilidad`. Usa `srgb8_to_linear`, inversa exacta de `to_rgb8`, comprobada en los 256 códigos.

Qué se ve con este compositor:
- Lo que está delante de la criatura (nave o planeta) queda intacto.
- Lo que está detrás (estrellas, planetas o la nave) se ve a través de ella.

Límites:
- **Sobre el espacio casi negro, la mezcla se percibe sobre todo como atenuación.** La translucidez solo se distingue donde hay estrellas, planetas o la nave detrás. Al mezclar en lineal, el brillo percibido crece rápido: con visibilidad 0.68 (tecla `3`) queda cerca del 85 % del de la estática (≈ 0.68^(1/2.4)).
- **No es transparencia física de varias capas.** Las partes internas de la criatura no se ven a través de su superficie frontal.
- **Tiene que ser lo último que se dibuja en 3D:** el z-buffer queda con la profundidad de la criatura. Una entidad que en el futuro se dibuje después la trataría como opaca. No es una transparencia general ni corrige la profundidad para otras entidades.
- **Contadores:** son los del cuadro sin criatura más los de dibujar la criatura al final. La mezcla no suma escrituras. Los contadores aditivos son iguales a los de la estática; los fragmentos y los píxeles escritos cambian porque cambia el orden.
- **Frustum:** descartada por el frustum, se cuenta como la estática descartada y no copia nada.
- **Buffers:** dentro de `World`, en un `RefCell`, y reutilizados entre cuadros. Siguen la resolución del framebuffer y no se reservan con visibilidad 0 o 1 ni con la criatura descartada. Ocupan unos 5 bytes por píxel (copia de color y marca) más 4 por píxel cubierto: unos 4.6 MB a 1280×720 más la máscara. `World::creature_reveal_buffer_bytes` informa del total.
- **Costo:** con visibilidad parcial se rasteriza lo mismo que la criatura estática, más la copia y la mezcla. **No está medido.**

### Pruebas (`tests/creature_reveal.rs`, 17)

Todos los fixtures son sintéticos (un cubo y dos cuadrados apilados de colores únicos); no hace falta el asset ni se abren ventanas.

- **Ajustes y objetivo:** validación, valores de las teclas, monotonía, recorte y valores no finitos.
- **Paso temporal:** 95 % en los segundos configurados, llegada exacta a los extremos, monotonía sin pasarse, composición en N trozos, un cuadro de 0.25 s sin límite y entradas no finitas.
- **Mezcla:** inversa sRGB exacta y mezcla lineal (distinta de mezclar en sRGB).
- **`World`:**
  - **Extremos:** sin modo, todo queda igual; oculta es igual a no tener criatura y entera es igual a la estática, con píxeles y contadores.
  - **Transiciones:** usan tiempo real, sin `update` y sin `F5`.
  - **Saltos y cambios de criatura:** los recolocados conservan el O₂ y saltan al objetivo; quitar y poner la criatura funciona como se describe arriba.
- **Compositor:**
  - En cada píxel, el resultado es exactamente la mezcla lineal del cuadro sin criatura con el de la criatura opaca.
  - Las caras superpuestas se mezclan una sola vez.
  - La nave y un planeta, delante o detrás, se comportan como se describe arriba.
  - Los contadores no suman escrituras de la mezcla.
  - Los buffers solo existen en la visibilidad parcial visible, siguen la resolución y se reutilizan.
- **Binario** (siempre `--headless`): combinaciones inválidas rechazadas sin crear PNG, protección de `--output`, parcial al 68 %, oculta con 5 objetos, entera idéntica byte a byte a la estática sin mover la nave, y `--debug-oxygen` sin criatura idéntico al juego normal.

## Revelación por etapas (`--creature-reveal-stages`)

Candidata opt-in, con la **variante B** que eligió Charlie («ojo con un poco de cuenca»): primero aparece el ojo y después el cuerpo. La revelación uniforme (`--creature-reveal`) se conserva tal cual como otra variante. Los dos modos son excluyentes.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --near-creature --creature-reveal-stages [--debug-oxygen 0.25]
```

| Flag | Contrato |
|---|---|
| `--creature-reveal-stages` | Requiere `--creature-model` y `--creature-texture`. Se rechaza con `--legacy` y con `--creature-reveal` («elige un modo»). **No** implica `--near-creature` ni mueve la nave |

- `--debug-oxygen`, las teclas `1`–`4` y `M`/`N` funcionan igual que con la revelación uniforme.
- Las combinaciones se validan antes de leer archivos.
- La región del ojo se valida al cargar: **falla cerrada** si no toma ninguna cara o las toma todas.

### Identidad del ojo (`src/visual/staged_creature_reveal.rs`)

- **`CREATURE_EYE_REGION`:** son del ojo **todas las caras** cuyo centroide, en espacio de objeto del OBJ, queda a menos de **0.20** de **(0.049, 0.059, 0.311)**. No hay clasificación por color.
  - Con el asset actual da 291 caras de ojo y 30 805 de cuerpo.
  - **Está calibrada para este asset Meshy:** otro modelo o un remesh exigen recalibrar.
- **`StagedParts::split`** parte el modelo **una vez**: al activar el modo o en `set_creature`.
  - Las dos partes copian vértices, UV, normales, materiales y bounds originales, así que el transform es exactamente el de la criatura. Las caras se reparten sin solaparse y sin perder ninguna.
  - No guardan textura: dibujan con la de la `StaticCreature` original, prestada.
  - **Memoria de la caché:** dos copias de los arreglos más las caras repartidas. Con el asset actual, `cache_bytes()` informa 7 079 504 bytes (≈ 7.1 MB). Es una duplicación consciente, sin optimizar.

### Etapas (candidatas, no aprobadas)

`StagedRevealSettings` se valida al construirse:
- `0 ≤ full < hint < eye_full < start ≤ 1`.
- `hint_alpha` en `[0, 1)`.
- Tiempos como en V1.

| O₂ | Ojo | Cuerpo |
|---|---|---|
| ≥ 50 % (`start`) | 0 | 0 |
| 50 → 30 % (`eye_full`) | 0 → 1 (smoothstep) | 0 |
| 30 → 25 % (`hint`) | 1 | 0 → **0.02** (smoothstep): contorno muy tenue |
| 25 → 5 % (`full`) | 1 | 0.02 → 1 (smoothstep) |
| ≤ 5 % | 1 | 1: ruta estática exacta |

- **Un único progreso normalizado** `p` en `[0, 1]` sigue al O₂ de forma lineal por tramos. Los dos alfas salen de `p`, así que **el cuerpo solo es mayor que 0 con el ojo exactamente en 1**, también mientras transiciona (al aparecer y al recuperar O₂).
- **Transición:** `p` avanza con `step_reveal` de V1, 2 s al aparecer y 0.7 s al ocultarse (95 %), con tiempo real. La aplica `World::update_creature_reveal`, como en V1.
- **Saltos al objetivo:** `R`, `place_near` y `set_creature` conservan el O₂ y saltan a su objetivo.
- **Alfas:** son **opacidad lineal** del compositor, no brillo percibido. Sobre el espacio casi negro, un 2 % lineal se percibe como algo más: `≈ 0.02^(1/2.4)` ≈ 20 % del brillo.

### Render

| Estado | Qué se dibuja |
|---|---|
| Ambos en 0 | Nada: píxeles y `RenderStats` idénticos a no tener criatura |
| Ambos en 1 | La ruta estática de siempre (antes de la nave): píxeles y contadores exactos |
| Parcial | **Después** de cuerpos, estrellas y nave: el ojo con `RevealCompositor::render_part` y, solo con el ojo opaco, el cuerpo |

Lo que garantiza ese orden en el parcial:
- **Con el cuerpo en 0, el cuerpo no se dibuja.** La banda púrpura que está delante del ojo no abre una raya negra; el ojo se ve completo.
- **Con el cuerpo a medias:**
  - El cuerpo que está **delante** del ojo se mezcla **sobre el ojo opaco**.
  - El cuerpo que está **detrás** queda tapado por el ojo.
  - Dentro del cuerpo sigue habiendo una sola capa, como en V1.
- **Lo último en 3D:** igual que V1.

Descarte, contadores y buffers:
- **Descarte:** se usa la esfera de la **criatura completa** antes de tocar las partes. Si queda fuera, se cuenta como la estática descartada y no se copia nada. Respeta `--no-culling` y F3.
- **Contadores:** **una** criatura lógica (`objects_submitted`). Los vértices, triángulos y fragmentos son los que dibujan realmente las partes: con las dos, los vértices del modelo cuentan dos veces porque cada parte los transforma. La mezcla no suma escrituras.
- **Buffers:** los del compositor de V1, reutilizados entre cuadros.

### Compositor generalizado (`src/visual/creature_reveal.rs`)

- `RevealPart { model, transform, texture }` es una vista prestada.
- `RevealCompositor::render_part` hace lo siguiente según alfa:

  | Alfa | Resultado |
  |---|---|
  | 0 | Nada |
  | 1 | Opaco, igual que `StaticCreature::render` |
  | Parcial | El mismo fantasma de V1 |

- Así se dibujan subconjuntos sin copiar la textura ni tocar el renderer.
- `RevealCompositor::render` mantiene su firma y su comportamiento exactos.

### Pruebas (`tests/staged_creature_reveal.rs`, 17)

Todos los fixtures son sintéticos.

- **Ajustes:** valores hostiles y por defecto.
- **Progreso:** extremos, teclas (25 % → ojo 1 y cuerpo 0.02), monotonía, nunca cuerpo con ojo parcial (también al transicionar), tiempo real como V1.
- **ROI y partición:** validación de la ROI, partición disjunta y completa sin tocar la geometría, y fallo cerrado.
- **Compositor:** `render_part` igual a las rutas de V1.
- **`World`:**
  - Extremos exactos.
  - Fase del ojo sin raya.
  - Contorno al 25 % y cuerpo sobre el ojo.
  - Contadores de una criatura lógica.
  - Descarte con y sin culling.
- **Ciclo de vida:** caché, fallos que no cambian nada, exclusión con V1, saltos al objetivo con `R`, buffers.
- **Binario:** validación, fallo cerrado de la ROI con otro modelo, 25 % con 6 objetos, 100 % con 5, y 5 % idéntico a la estática.

## Mirada local (`--creature-eye-follow`)

Opt-in. El ojo de la criatura mira a la **nave** (nunca a la cámara) dentro de sus límites; el cuerpo y los tentáculos no se mueven y la criatura entera nunca gira. La deformación es la de V02 que aprobó Charlie, sin cambios de arte.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --near-creature --creature-reveal-stages --creature-eye-follow [--debug-oxygen 0.05]
```

| Flag | Contrato |
|---|---|
| `--creature-eye-follow` | Requiere `--creature-reveal-stages` (que a su vez requiere la criatura). **No** activa las etapas por su cuenta. Se rechaza con `--legacy`. Con `--creature-reveal` y `--creature-reveal-stages` juntos gana el error heredado («elige un modo») |

- Las combinaciones se validan antes de leer archivos (`validate_eye_follow_flags`, después de `validate_staged_flags`).
- La calibración se hace al cargar: **falla cerrada** (código 1, sin PNG) si el modelo no se puede calibrar con la región del ojo de las etapas.
- No hay teclas nuevas. `1`–`4`, `M`/`N`, `F1`, `F3`–`F6`, `P` y `R` siguen igual.

### Núcleo (`src/visual/creature_eye_follow.rs`)

| Pieza | Contrato |
|---|---|
| Límites | Elipse yaw ±10° / pitch ±8° (`EYE_FOLLOW_MAX_YAW`, `EYE_FOLLOW_MAX_PITCH`): las poses aprobadas de V02. La elipse en diagonal es una decisión técnica candidata |
| Desvanecido | Seguimiento completo hasta 75° entre el eje óptico y la nave, reposo desde 105° y `smoothstep` en medio: de lado o por detrás vuelve al reposo sin saltos |
| `EyeFollowSettings` | Solo el tiempo hasta el 95 %, en `[0.01, 60]` s. Por defecto 1.5 s: **candidato**, sin aprobación visual |
| `EyeSkin::calibrate` | Una vez, sobre la pose de reposo: las mismas caras de ojo que `StagedParts::split`, pesos continuos de V02, pivote y eje óptico. Calcula `posed_bounds_radius`, una envolvente válida para cualquier pose |
| `EyePoseCache` | Solo las caras del ojo, posadas **en sitio** desde el reposo, sin reservar memoria por pose. Ante un error no escribe nada |

### En el mundo (`World`)

| API | Contrato |
|---|---|
| `enable_creature_eye_follow(settings)` | Exige el modo por etapas activo (si no, `Err`). Con criatura calibra **ahora**; si falla devuelve `Err` y el mundo queda como estaba. Sin criatura se activa igual (como las etapas) y solo guarda el estado angular. Arranca ya mirando a la nave (sin transición) |
| `disable_creature_eye_follow()` | Suelta la calibración y la caché. Las etapas vuelven a dibujarse exactamente como sin mirada y el O₂ no cambia |
| `update_creature_eye_follow(segundos)` | Paso con **tiempo real**, como `update_creature_reveal`: sin el límite del `dt` físico, sin sumar al tiempo orbital y sin que `F5` lo congele. Persigue la posición actual de la nave. Avanza aunque la criatura esté oculta (solo ángulos) |
| `creature_eye_follow()` | Estado angular (ajustes y ángulos actuales) |
| QA | `creature_eye_skin()`, `creature_eye_posed()`, `creature_eye_pose_updates()` y `creature_eye_cache_bytes()`: solo lectura, sin HUD |

- **Saltos al objetivo:** al activar, en `set_creature`, `R` (`reset_ship` o `place_near_creature`) y `place_near`, siempre **después** de colocar la nave. Conservan el O₂.
- **Arranque headless:** salta al objetivo, así que la captura es determinista. Con el asset real y `--near-creature` el ojo ya mira unos (−2.87°, −2.09°) hacia la nave, porque el pivote del ojo no está en el centro de la criatura. Es la consecuencia lógica de mirar a la nave, no otra dirección artística.
- **Cambios de modo:** la mirada vive **dentro** del modo por etapas.
  - `enable_creature_reveal` (V1), `disable_staged_creature_reveal` o volver a llamar a `enable_staged_creature_reveal`, aunque sea con la misma región, la apagan. Hay que volver a activarla, así nunca queda una calibración de otra región.
  - Volver a activarla sustituye los ajustes.
- **Criatura:**
  - `set_creature` calibra la nueva criatura **antes** de cambiar nada, junto con la partición. Si falla, la criatura, la partición, la calibración y el estado anteriores siguen intactos.
  - `clear_creature` suelta la calibración y la caché, pero conserva el estado angular. Sin criatura no se dibuja nada.
- **Coste:** la calibración (Dijkstra, ajuste de esfera, partición) y la carga se hacen una sola vez. Por cuadro solo se calcula el objetivo, que es un punto transformado.

### Render

La geometría se posa **de forma perezosa al dibujar**, en una caché dentro de un `RefCell`, como el compositor. Solo se posa si los ángulos cambiaron desde la última pose, y nunca con la criatura oculta (ojo en 0) ni descartada por el frustum. Repetir el cuadro no acumula nada.

| Estado | Qué se dibuja |
|---|---|
| Sin mirada, o mirada en reposo exacto con `fits_bind_sphere()` | Las rutas de etapas de siempre, **exactas** (píxeles y `RenderStats`) |
| Ojo y cuerpo en 0 | Nada: igual que sin criatura. No se posa |
| Ambos en 1 (entera, 5 %) | El ojo posado opaco y el cuerpo de reposo opaco (`StagedParts::body`), **antes** de la nave |
| Parcial | Después de la nave: el ojo posado con su alfa (también de 50 % a 30 %) y, solo con el ojo en 1 y el cuerpo positivo, el cuerpo de reposo al final, como en las etapas |

- **Una sola partición:** el ojo posado usa las mismas caras que la partición B de las etapas, sin reclasificar vértices posados. Las dos partes usan el transform y la textura (prestada) de la criatura original.
- **Descarte:** con la mirada posada se usa la envolvente posada en mundo, `posed_bounds_radius · radio_mundo / bounds_radius` (nunca menor que la esfera original), **antes** de posar o transformar nada.
  - Descartada: cuenta 1 objeto enviado y descartado, y todas sus caras como enviadas y descartadas, con 0 vértices.
  - Si la envolvente no cabe en la esfera original (`!fits_bind_sphere`), se dibuja por esta ruta incluso en reposo, para no fingir una esfera insuficiente.
  - Con el asset real la envolvente es igual al radio de reposo (1.120858), así que el descarte no cambia.
- **Contadores:** **una** criatura lógica. Vértices, triángulos y fragmentos son los reales: el ojo posado y el cuerpo procesan cada uno el arreglo de vértices completo, así que con las dos partes se cuentan dos veces.
- **Fidelidad:** el padre comparó píxel a píxel la combinación ojo posado + cuerpo de reposo con el modelo completo posado de V02, con el asset real: 18 comparaciones idénticas. Esa prueba dibujaba la criatura después de la nave; el orden real del mundo lo cubren las pruebas de abajo.

### Pruebas (`tests/creature_eye_follow_world.rs`, 14)

Todos los fixtures son sintéticos: una cúpula calibrable (sola, o con un triángulo de cuerpo desplazado para tener bounds descentrados como el asset real) y una textura de 2×2.
- **Flags:** matriz pura de combinaciones.
- **Ciclo de vida:** exigir las etapas; activar sin criatura; recalibrar al cambiar de asset; fallos sin cambios (`set_creature` y `enable`); `clear`; cambios de modo; desactivar devuelve las etapas exactas sin tocar el O₂.
- **Tiempo:** objetivo en la nave y no en la cámara; tiempo real, independiente de `update`, `F5` y el tiempo orbital; saltos al objetivo con `R` y `place_near`.
- **Render:**
  - Entera y parcial iguales píxel a píxel y en contadores a una referencia hecha con la API pública.
  - Reposo idéntico a las etapas.
  - Envolvente expandida y descarte, también con un caso en que la esfera original descartaría por error.
  - Pose perezosa: nada oculta ni descartada, una pose por ángulo y la memoria constante.
  - Z real con la nave visible: entera antes y parcial después. Una mutación temporal confirmó que el test detecta el orden equivocado.
- **Binario** (siempre `--headless`): rechazos antes de IO con rutas inexistentes, protección de `--output`, modelo que no calibra, arranque ya mirando a la nave al 5 % y el cuadro oculto idéntico al de las etapas sin mirada.

### Comparación nave/cámara (`--creature-eye-compare`)

Prueba A/B opt-in, **no** promoción: solo cambia el punto al que mira el ojo, para aislar el objetivo. Los límites (10°/8°), el desvanecido (75°→105°), el tiempo (1.5 s), los pesos y la geometría son los mismos. La criatura entera nunca gira.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --near-creature --creature-reveal-stages --creature-eye-follow --creature-eye-compare
```

| Pieza | Contrato |
|---|---|
| `--creature-eye-compare` | Requiere `--creature-eye-follow` (y por la validación previa, las etapas y la criatura; no V1). Se rechaza con `--legacy`. Se valida antes de leer archivos, después de los flags anteriores, cuyos errores se conservan. Sin el flag, todas las rutas, mensajes y PNG son los de antes |
| `EyeTarget::{Ship, Camera}` (`src/visual/creature_eye_comparison.rs`) | `Ship` por defecto. `Camera` es la **posición** de la cámara de persecución, nunca el punto al que mira |
| `World::set_creature_eye_target(t)` | Solo cambia la elección: no recalibra, no toca la caché ni la pose y no salta los ángulos; `update_creature_eye_follow` llega al nuevo objetivo con la transición de siempre. Sin mirada, `Err` sin efectos |
| `World::enable_creature_eye_follow_targeting(s, t)` | Como `enable_creature_eye_follow` (que equivale a `t = Ship`), con salto inicial al objetivo elegido |
| `creature_eye_target()`, `creature_eye_goal()`, `creature_eye_comparison_status()` | Elección, objetivo angular actual y lo que muestra el panel (QA/HUD) |
| `app::run_game_with_eye_comparison(options)` | Ruta opt-in. Valida antes de cualquier IO; `run_game` no cambia |

- **Ciclo de vida:**
  - `clear_creature` y `set_creature` conservan la elección.
  - `R`, `place_near` y `place_near_creature` la conservan y saltan al objetivo elegido **después** de recolocar la nave y la cámara.
  - Apagar y volver a activar la mirada vuelve a `Ship`. Cambiar de modo de revelación la apaga como antes.
- **Ventana:** arranca mirando a la cámara. `G` (sin repetición) alterna NAVE/CÁMARA en vuelo; el tiempo, la transición, la caché y el O₂ siguen igual. Las demás teclas no cambian.
- **Panel inferior** (abajo a la izquierda, separado de la banda superior del overlay, que no cambia):
  - Se dibuja con `draw_text` y `darken_rect` existentes, también en headless y con `F1` apagado, solo en la comparación.
  - Usa la misma escala que el overlay: 1 bajo 1000 px de ancho y 2 desde ahí. Las constantes 5×7 de la fuente se replican porque son privadas de `overlay.rs`.
  - Muestra el objetivo (`MIRADA A: CAMARA   G CAMBIA A NAVE`), los ángulos actuales, el objetivo angular, el estado y el ángulo al eje:
    - `DENTRO`: dentro de la elipse.
    - `TOPE`: en el borde de la elipse 10/8.
    - `DESVANECE`: entre 75° y 105° del eje.
    - `REPOSO`: desde 105°.
    - `SIN DIRECCION`: sin dirección al objetivo.
  - El texto lo calcula el helper puro `eye_comparison_lines`.
- **Lo que no promete:**
  - Arriba y abajo, el pitch satura en 8° desde unos 10° de elevación y se desvanece de 75° a 105°, apunte a la nave o a la cámara. Elegir la cámara no quita esos topes.
  - La cámara va 22 detrás y 6 arriba de la nave: en una trayectoria de prueba de 90 cuadros la diferencia entre ambos objetivos fue de 2.26° como máximo.

Pruebas (`tests/creature_eye_comparison.rs`, 11, fixtures sintéticos):
- **Elección:** identidad del objetivo con la nave y la cámara en sitios distintos, sin pose nueva ni cambios de caché al alternar.
- **Ciclo de vida:** `clear`/`set`, `R`/`near` y vuelta al default.
- **Diagnóstico puro:** ángulo al eje y estado consistentes con `aim_gaze`, con los datos arriba/abajo.
- **Panel:** en 320×180, 640×360 y 1280×720; solo toca su rectángulo.
- **CLI:** validación antes de IO y protección de `--output`; arranque mirando a la cámara y panel en headless, también con `--overlay`.
- **Trayectoria real:** controles → física → cámara → mirada → render, con los dos defaults exactos.

### Giro global lejano (`--creature-distant-turn`, candidata)

Experimento opt-in en una **candidata aislada**, no la integración principal ni el default final. De lejos el ojo no se distingue, así que la criatura entera gira, lenta y limitada, hacia el punto elegido con `G`. De cerca se queda quieta y solo mira el ojo, con la mirada aprobada. Los parámetros son **técnicos candidatos**, sin aprobación artística. La geometría V02, el iris, las UV, la textura y los pesos no cambian.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --creature-reveal-stages --creature-eye-follow --creature-eye-compare --creature-distant-turn
```

| Pieza | Contrato |
|---|---|
| `--creature-distant-turn` | Requiere `--creature-eye-compare`, y por las validaciones previas la mirada, las etapas y la criatura (no V1). Se rechaza con `--legacy`. Se valida antes de leer archivos y después de los flags anteriores, cuyos errores se conservan; mantiene la protección de alias de `--output`. Sin el flag, todo es como antes |
| `app::run_game_with_distant_turn(options)` | Entrada de biblioteca con los mismos guards que la comparación, antes de cualquier IO. `GameOptions`, `run_game` y `run_game_with_eye_comparison` no cambian |
| `TurnAngles` (`src/visual/creature_distant_turn.rs`) | Yaw máx. 20° y pitch máx. 12°, con envolvente **elíptica**. Convención: yaw positivo lleva el frente (+Z del OBJ) hacia +X del OBJ; **pitch positivo, hacia arriba** |
| `aim_turn(centro, yaw, objetivo)` | Pedido desde el **centro fijo** en el marco de **reposo** (no se realimenta). Usa un mapa ángulo-dirección como `aim_gaze`, continuo, sin el salto ±180° de un Euler con `atan2`. Se recorta a la elipse y se multiplica por el **desvanecido 75°→105°** del ojo (reutiliza las constantes; `creature_eye_follow` no cambia): nunca da la vuelta hacia algo que está detrás |
| `distance_weight(d)` | `d` = distancia de la **cámara** al centro en radios. Peso 0 hasta 3, 1 desde 7 y smoothstep en medio; no finito da 0 |
| `DistantTurn` | Encendido/apagado (`H`) y ángulos actuales. Persecución exponencial con **t95 = 8 s** reales (el ojo, 1.5 s), independiente del troceo; avanza con el tiempo real y `F5` no lo congela. Apagado, el objetivo es el reposo y vuelve con la misma transición (sin salto) |
| `turned_euler` / `turned_transform` | `R = Ry(yaw_criatura + yaw) · Rx(−pitch)`, Euler `(−pitch, yaw_criatura + yaw, 0)`, sin alabeo. La traslación deja el **centro envolvente del OBJ** en `placement.center()`, con bounds descentrados y `yaw` de colocación ≠ 0. Misma escala; normales rígidas por `Transform`. En reposo devuelve el transform original **exacto** |
| `world_turn` / `world_turn_inverse` / `rest_frame_target` | Giro en mundo `G = Ry(py+yaw)·Rx(−pitch)·Ry(−py)` alrededor del centro y su inversa **analítica** (mismos factores invertidos). El ojo apunta con su calibración de reposo a `centro + G⁻¹(objetivo − centro)`: no se suman dos miradas ni cambia `creature_eye_follow`. En reposo, el objetivo exacto |
| `CreaturePreset::{Near, Far}`, `preset_pose` | `Near` = `spawn_pose` (2.5 radios, de frente). `Far` = 8 radios y 25° laterales hacia +X del frente, mirando al centro y sin elevación |
| `World::enable/disable_creature_distant_turn`, `set_creature_distant_turn_active`, `creature_distant_turn(_status)`, `place_creature_preset` | Exige la mirada (`Err` sin efectos). Desaparece con la mirada (apagarla, reiniciar o cambiar el modo de revelación). `clear`/`set` de la criatura conservan los ángulos y el ojo salta al punto compensado. Los presets dejan el giro en reposo exacto y saltan el ojo contra la cámara ya colocada, conservando O₂, `G` y `H` |

- **Orden por cuadro:** física y cámara (`update`) → `update_creature_eye_follow(elapsed)`, que avanza primero el giro hacia el mismo punto de `G` (posición de la nave o de la **cámara**, nunca `camera.target`) y después el ojo hacia el objetivo compensado.
- **Render:**
  - Con el giro fuera del reposo, ojo **y** cuerpo usan el **mismo** transform girado, también con el ojo en reposo; nunca se cae a `StaticCreature::render`, que lo ignoraría.
  - El descarte usa la esfera de siempre centrada en `placement.center()` (la completa o la envolvente posada del ojo), antes de posar; la rotación rígida no la agranda.
  - Oculta por O₂ o fuera del frustum, no posa.
  - Opaca antes de la nave; parcial al final del 3D con el compositor actual.
  - En reposo, las rutas de siempre exactas.
  - No copia ni toca vértices, normales, UV ni caras, y no recarga modelo, textura ni skin.
- **Ventana:** arranca en el preset **LEJOS**, aunque se pida `--near-creature`.
  - `H` alterna GIRO GLOBAL ON/OFF (apagado vuelve suave al reposo y el panel lo avisa).
  - `7` coloca CERCA, `8` coloca LEJOS y `R` repite el último preset.
  - `G` alterna NAVE/CÁMARA. Las demás teclas no cambian.
- **Panel:**
  - El de la comparación sigue abajo a la izquierda; su objetivo, su estado TOPE/fade y el eje van en el **marco compensado**.
  - El del giro va abajo a la derecha, o encima del otro si no caben: `H`, presets, distancia y peso, giro actual y objetivo, y estado del pedido (DENTRO/TOPE/DESVANECE/REPOSO) con su ángulo al frente.
- **Lo que no promete:**
  - Al acercarse tras girar lejos, el cuerpo **vuelve visiblemente** (t95 8 s), no se congela al instante.
  - De lejos el giro es sutil a 640×360 (la criatura ocupa ~60 px a 8 radios).

Pruebas (`tests/creature_distant_turn.rs`, 24 + 1 `#[ignore]` con el asset real por variables de entorno):
- Núcleo puro y estado temporal.
- Transform rígido con `yaw` ≠ 0 y bounds descentrados, con su inversa.
- Ciclo de vida y presets.
- Avance y compensación; cerca, la mirada sale bit a bit igual.
- Rutas de render: reposo exacto, ojo en reposo con giro, descarte y O₂.
- Paneles, CLI y biblioteca.
- Replay pilotado lejos→cerca→lejos y alternancia de presets.

Preview: `examples/creature_distant_turn_preview.rs` (mundo real, matriz declarada en su encabezado).

### Seguimiento corporal horizontal (`--creature-horizontal-follow`, candidata 02)

> **Estado actual:** aprobado en vivo por Charlie y congelado (t95 4 s, tope 18°/s). La ruta del [Sol con ojo](#sol-con-ojo-en-el-juego-creature-solar-reveal) lo usa sin cambios. El texto de esta sección es el de la candidata.

Experimento opt-in en una **candidata aislada** (no es la integración principal, la colocación en el Sol ni el default final). Cerca y lejos, la criatura entera gira **solo en horizontal** (alrededor de +Y del mundo, sobre su centro fijo) hacia el punto elegido con `G`, en **todo el azimut**, lenta y con rapidez limitada. Sustituye, como opción separada, al giro global de la candidata 01 (que se conserva intacto y no se apila con este). El ojo aprobado conserva su deformación, límites 10/8 y desvanecido.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --creature-reveal-stages --creature-eye-follow --creature-eye-compare --creature-horizontal-follow
```

| Pieza | Contrato |
|---|---|
| `--creature-horizontal-follow` | Requiere `--creature-eye-compare` (y por las validaciones previas, la mirada, las etapas y la criatura; no V1). Rechazado con `--legacy` y con `--creature-distant-turn`. Se valida antes de leer o escribir archivos, con la protección de alias de `--output`. Sin el flag, todo es como antes |
| `app::run_game_with_horizontal_follow(options)` | Entrada de biblioteca con los guards de la comparación, antes de cualquier IO. `GameOptions` y las entradas anteriores no cambian |
| `target_azimuth(centro, yaw, objetivo)` | Azimut en `(-π, π]` de la **proyección XZ** del objetivo vista desde el centro, en el marco de reposo (0 = frente +Z del OBJ, positivo hacia +X del OBJ). La altura no cuenta. `None` sin dirección XZ (sobre/bajo el centro, coincidente) o no finito |
| `HorizontalFollow` | Giro actual en `(-π, π]` (acotado tras cualquier número de vueltas), último objetivo válido, sentido del último giro y rapidez real. Ley `|e|' = −min(k·|e|, vmax)` por el **arco corto**, con `k = ln 20 / t95`: candidata **t95 = 4 s, vmax = 18°/s**. Solución exacta con el objetivo fijo (independiente del troceo, sin pasarse, rapidez ≤ vmax). Segundos **reales** (`F5` no lo congela) |
| Casos límite | Empate antipodal exacto: el sentido del último giro (positivo al principio). Objetivo sin dirección XZ: conserva el último objetivo válido (nunca vuelve al frente por eso). `elapsed` inválido no avanza |
| `H` | ON persigue el último objetivo; OFF lleva el objetivo efectivo al **frente de reposo** con la misma dinámica (no es una pausa) y el panel lo dice. Nunca salta |
| `horizontal_transform(criatura, yaw)` | Exactamente el transform de una colocación con yaw `placement.yaw() + yaw` en el mismo centro (misma fórmula que `transform_for`): Euler `(0, yaw total, 0)`, misma escala, centro envolvente del OBJ fijo. Ojo y cuerpo usan el **mismo** transform. Con 0, el original exacto |
| `rest_frame_point(centro, yaw, p)` | `centro + Ry(−yaw)(p − centro)`: el ojo apunta con su calibración de reposo a ese punto (inversa analítica, sin doble giro ni cambio del skin) |
| `World::enable/disable_creature_horizontal_follow`, `set_creature_horizontal_follow_active`, `creature_horizontal_follow(_status)`, `creature_render_transform` | Exige la mirada; excluyente con el giro global de la 01 en los dos sentidos (`Err` sin efectos). Muere con la mirada. `clear`/`set` conservan el giro. `place_creature_preset` lo deja de frente exacto y sin objetivo previo |

- **Orden por cuadro:** física y cámara (`update`) → `update_creature_eye_follow(elapsed)`, que avanza primero el cuerpo hacia la proyección XZ del punto de `G` (posición de la nave o de la **cámara**, nunca `camera.target`) y después el ojo hacia el punto compensado. Sin peso por distancia: sigue cerca y lejos.
- **Render:** rutas de la 01 con el transform horizontal: descarte con la esfera de siempre (la rotación rígida no la agranda) antes de posar; ojo y cuerpo girados también con el ojo en reposo; opaca antes de la nave, parcial al final; oculta por O₂ no posa. En reposo (giro 0), rutas antiguas exactas.
- **Ventana:** arranca en el preset LEJOS mirando a la cámara (también con `--near-creature`); el log de la mirada sale **después** del preset. `H` ON/OFF, `7` CERCA, `8` LEJOS, `R` repite el último preset (conserva O₂, `G` y `H`). Panel derecho con regla y rapidez reales (sin los 20/12 ni el fade de la 01).
- **Lo que no promete:** retraso estable al rodearla (≈ `ω·t95/ln 20`): unos 7° a 8 radios y 20° a 2.8 radios a crucero; a más de 18°/s (impulso cerca) el cuerpo se queda atrás y, si el retraso pasara de 180°, el arco corto invertiría el sentido. Sin giro vertical.

Pruebas (`tests/creature_horizontal_follow.rs`, 22 + 1 `#[ignore]` con el asset real por variables de entorno). Preview: `examples/creature_horizontal_follow_preview.rs` (mundo real, matriz declarada en su encabezado).

## Sol con ojo en el juego (`--creature-solar-reveal`)

**Estado:** la composición del preview 03 está **aprobada visualmente por Charlie** («Yo creo que está re bien, tamaño, posición, todo, me gusta») e integrada en el bucle principal como opción explícita, conservando la composición y la vuelta atrás. La aprobación es de la composición, no del vuelo ni de los controles con teclas físicas, que quedan para la prueba manual de Charlie. El lanzador solar local utiliza esta entrada del proyecto principal con assets externos; los lanzadores anteriores se conservan. Sin assets ni el flag, el arranque genérico no cambia.

```bash
cargo run --release -- --creature-model ruta/modelo.obj --creature-texture ruta/textura.png --creature-solar-reveal
```

| Pieza | Contrato |
|---|---|
| `--creature-solar-reveal` | Requiere el modelo y la textura **externos**, por CLI: la fuente no declara rutas privadas ni incluye assets. Implica las etapas, la mirada (a la cámara), la comparación (`G`) y el seguimiento horizontal 02; esos flags pueden repetirse sin efecto. **Rechaza antes de leer o escribir archivos:** `--legacy`, `--creature-reveal` (V1), `--near N`, `--near-creature`, `--creature-distant-turn`, y un `--output` que no sea `.png` o que resuelva a una entrada |
| `app::run_game_with_solar_reveal(options)` | Entrada de biblioteca con los mismos guards, antes de cualquier IO. `GameOptions` no cambia (sus literales siguen compilando); `creature_reveal_stages` y `creature_eye_follow` se ignoran porque van implícitos. `run_game` y las entradas `run_game_with_*` anteriores no cambian |
| `visual::solar_game` | Envoltorio **sin algoritmos nuevos** sobre las APIs aceptadas de `World`. `solar_creature_placement`: centro del Sol, radio 80, yaw 0. `SolarGame::start`: la receta del preview en su orden (criatura, etapas, mirada a la CÁMARA, seguimiento 02, `enable_solar_reveal_preview(Camera)` y vista FRONTAL). `SolarGame::render`: siempre `World::render_solar_reveal_preview`. `SolarGame::apply`: las teclas. `SolarGame::title`: el título |
| `app::advance_frame(world, input, elapsed)` | El orden por cuadro de **todos** los modos, sin cambios: `update` con `dt ≤ 0.1` (`MAX_PHYSICS_DT`) y después la revelación y la mirada con los segundos reales. La ventana y las pruebas usan la misma función |

- **Composición:** idéntica a la aprobada. La misma criatura en el centro del Sol, radio 80 y yaw 0; las etapas 50/30/25/5, con el cuerpo a 0.02 al 25 %; el ojo V02; el seguimiento horizontal 02 (t95 4 s, 18°/s); el mismo `SolarPairCompositor` y la luz local de cámara. No cambian assets, UV, normales, shaders, renderer, matemáticas, nave, cámara, carga ni el código de `World` o del compositor.
- **Arranque:** vista FRONTAL (`place_solar_reveal_camera(0, 298, 0)`), mirada a la cámara y O₂ de depuración al 100 % por defecto (`--debug-oxygen` lo cambia). Una sola carga, una partición y una calibración.
- **Arranque frente a asentado:** no se congelan las órbitas ni se simulan los 20 s del preview. En la vista frontal el cuadro de arranque **ya es** el asentado; lo comprueban `tests/solar_game.rs` y los 8 PNG frontales aprobados. En la vista oblicua, el cuerpo y el ojo llegan con su dinámica, en segundos reales.
- **Teclas:** `7` FRONTAL, `8` OBLICUA (70°, 578, 60) y `R` vuelve a FRONTAL.
  - Las vistas saltan la revelación y la mirada al objetivo, como el preview, pero **no** el cuerpo: el seguimiento conserva su giro y lo continúa.
  - `H` enciende o apaga el seguimiento (apagado vuelve al frente con su dinámica) y `G` alterna CÁMARA/NAVE.
  - Todas conservan el O₂, el reloj orbital y las elecciones.
  - `1`–`4`, `M`/`N`, `P`, `F1`/`F3`/`F4`/`F5`/`F6` y el vuelo funcionan como siempre. `F5` solo congela las órbitas: la revelación, la mirada y el seguimiento siguen.
- **Tiempo y Sol:** las órbitas siguen vivas. El Sol no tiene órbita, así que la criatura sigue anclada a su centro, y su giro axial nunca arrastra a la criatura: el transform de la criatura solo depende del seguimiento.
- **Cuadro:** el inicial, el headless y cada cuadro del bucle pasan por `SolarGame::render`, nunca por `World::render` por accidente. Con el ojo oculto (O₂ ≥ 50 %) el resultado es exactamente `World::render`, por contrato.
- **Sin HUD por defecto:** el estado (O₂, `G`, `H` y vista) va en el título de la ventana. `F1` y `--overlay` siguen disponibles; no hay panel de comparación.
- **Sin el flag,** todo queda exacto: el modo sin assets, `--legacy` y las rutas anteriores, con sus controles.

Pruebas: `tests/solar_game.rs` tiene 15 pruebas y 2 `#[ignore]` con el asset real y los PNG aprobados, por variables de entorno (`UMBRAL_REAL_CREATURE_OBJ`, `UMBRAL_REAL_CREATURE_TEXTURE` y `UMBRAL_SOLAR_PREVIEW03_DIR`).

## Pendiente

- **Inspección visual y prueba manual de Charlie** en ventana (controles, `R` con `--near-creature`, legibilidad, y la revelación con las teclas `1`–`4`).
- **Decisiones de rendimiento** (LOD, mipmaps, malla reducida, ocultar detrás del Sol) y la **ubicación final** en el nivel.
- **Prueba nativa de la mirada** (ventana y controles físicos) y aprobación del tiempo candidato de 1.5 s. Animación e integración con la textura o el material de Persona 1. Integración de la revelación con el oxígeno real de UMB-205 y con `LucidSettings` (UMB-310).
- **Sol con ojo:** la prueba manual de Charlie en ventana (teclas físicas, vuelo y lectura en vivo). La composición aprobada ya está integrada y tiene un lanzador solar local; no se añade consumo automático de oxígeno ni colisiones.
- **Origen del modelo:** es externo, generado con Meshy AI. Su uso y su declaración dependen de la rúbrica y de los términos de Meshy, que no se han verificado aquí.
