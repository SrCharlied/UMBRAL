# Persecución viva del tentáculo (UMB-311, prototipo opt-in)

El brazo aprobado **persigue la posición actual de la nave** en la ruta solar del juego completo.

**Es un prototipo de persecución previa. NO es captura:**

- no hay agarre, arrastre, bloqueo de input, muerte, audio, apagón, explosión, cooldown ni colisiones;
- el tentáculo **nunca mueve la nave**;
- sin el flag, el juego no cambia.

El 0 % de O₂ sigue **indeciso** (muerte o captura): aquí solo se sostiene la rapidez del 5 %.

**Código:** `src/visual/tentacle_live_pursuit.rs`.

## Uso

```text
umbral --creature-model <Meshy.obj> --creature-texture <texture_1024_linear_box_diagnostic.png> \
  --creature-solar-reveal [--lucid-surface --void-eye --lucid-perception --watching-stars] \
  --tentacle-live-pursuit --tentacle-capture-package <capture_v2> \
  --tentacle-growth-package <growth_package.txt> --tentacle-arm-correspondence <arm_correspondence.txt>
```

- **Validación previa a cualquier IO:**
  - requiere `--creature-solar-reveal`;
  - es incompatible con `--tentacle-capture-debug` (F7) y con `--tentacle-capture-time`;
  - no admite `--legacy`;
  - exige las tres rutas.
- **Sin el flag:** las reglas de la composición observadora no cambian.
- **Protección de entradas:** las mismas que la composición (grafo SC01-MTL, FileID, RAII y preflight
  antes de cada guardado headless o `P`).
- **Controles:** los de siempre. El O₂ se cambia con `1`–`4` y `M`/`N`, y el estado va en el título. No hay
  F7 en este modo.

## O₂ y rapidez (valores CANDIDATOS, en unidades de mundo)

No son reglas aprobadas: son ajustes candidatos, a revisar con Charlie.

| O₂ mostrado | Comportamiento |
|---|---|
| > 15 % | Reposo: el cuadro es **exactamente** el de siempre (sin capa propia) |
| 15 % | Persigue a 12 u/s |
| 10 % | 24 u/s |
| 5 % | 48 u/s |
| 0 % | Sostiene la de 5 %: no hay acción nueva |

- **Entre bandas:** la rapidez es lineal.
- **Aceleración:** 48 u/s² fuera de la excepción de llegada, con frenado previsto `sqrt(2 a d)`.
  **Decisión expresa de Charlie:** «Frenar al llegar, sin pasarse, aunque deba frenar de golpe».
  La llegada exacta tiene prioridad sobre la cota provisional de desaceleración escalar cuando
  cambia el objetivo: se limita el avance a la distancia restante y se informa la rapidez real
  del paso. En el siguiente paso sin desplazamiento la rapidez es cero. No se liga ni mueve la nave.
  LP-R05 es un guard de preservación nacido GREEN, no una nueva función implementada en fix01.
- **Recuperación:** al subir el O₂ por encima de 15 %, la punta vuelve al reposo a 24 u/s y pasa a reposo
  al llegar **exacto**. No hay reset brusco.
- **De dónde sale el 5 %:** de la rapidez de pico de la preview observadora anterior en FRONTAL (F7 a 298
  del Sol): |Δ| 236,2 u, alcance 3,5 s × factor 2,1047 → pico 48,1 u/s (media 32,1 u/s).
- **Referencia:** la nave va a 60 u/s y a 300 u/s con boost.

## Controlador (puro)

`PursuitController` conserva la **punta en mundo** y su rapidez.

**Entradas de cada paso:**

- la posición **actual** de la nave (no la cámara ni una instantánea);
- el anclaje y la punta de reposo, con el transform **actual** de la criatura;
- el O₂ mostrado y el `dt` de la física (≤ 0,1 s).

**Cómo se mueve:**

- **Embestida recta** hacia la nave si el segmento punta → objetivo no entra en la bola mínima.
- Si entra, **camino polar** alrededor del anclaje: gira la dirección por el círculo máximo y cambia el
  radio a la vez, **rodeando** la bola.
- **Recorrido:** la longitud **exacta** de cada camino. El polar va en forma cerrada y su fracción se halla
  por bisección.
- **Antípoda:** usa el eje anterior o uno determinista.

**Bola mínima = radio de EXACTITUD de la malla:** `codo + sqrt(codo² + a²)`, ≈ 0,40 OBJ ≈ 29 u.

- Más cerca y con ángulo, el distal rígido no puede llevar su punta axial al objetivo: se desviaba hasta
  ~18 u, medido.
- La punta de reposo (`a`) está **dentro** de esa bola. Al perseguir, sale **radialmente por el eje de
  reposo** hasta el borde; es un tramo previo del mismo paso y el resto del presupuesto sigue hacia la
  nave sin perder rapidez. Al recoger, entra por la «puerta» del eje (el resto del paso sigue por el eje)
  y termina **exacta** en el reposo.
- Al empezar desde el reposo, parte de la punta de reposo **actual**: el cuerpo pudo girar. Si el giro
  del cuerpo deja la punta más cerca que `a`, la malla usa su proyección a `a` mientras el controlador la
  saca.
- **Alcance:** el radio máximo es 12 unidades OBJ (≈ 856 u). Fuera de él, el objetivo se recorta y el
  título lo dice.
- **Errores atómicos:** se rechazan NaN, infinitos, `dt` negativo o mayor que 1 s, O₂ fuera de [0, 1]
  y coordenadas mayores que 1e9. El estado no cambia.
- **`dt` = 0:** no mueve.

## Deformación (topología aprobada como dato)

`ShaftRig` usa la plantilla aprobada de 4801 vértices y 9585 caras. **No reexporta assets.**

- **Proximal y raíz:** inmóviles, en las posiciones **exactas** del Meshy (correspondencia). La costura
  con el cuerpo es **bit a bit**. La composición observadora usaba el export de 6 decimales (≤ 1e-6
  OBJ).
- **Tramo central:**
  - camino codo circular + recta (Dubins en el plano de la tangente del lazo y la punta);
  - radio del codo = 2 × el radio de contorno del lazo, candidato; deja un radio interior positivo;
  - anillos = copias **rígidas** del lazo aprobado, giradas alrededor de un eje fijo, así que no hay
    torsión;
  - el codo recibe una fracción mínima de anillos.
- **Distal y espiral:** rígidos, girados con la tangente final. Conservan anatomía, UV, material y
  grosor.
- **Punta perseguida:** el punto **axial** `centro distal + a · tangente`, donde `a` es la proyección del
  centroide del agarre (0,269 OBJ en el asset real).
- **Lado del codo:** el corto. Con el objetivo detrás se conserva el lado hasta 180° + 30° (histéresis
  candidata); el cambio se declara en `side_switched`.
- **Rechazos:** `Err` antes de escribir si la punta es no finita, está a menos de `a` o más allá del
  alcance, o si los buffers son de otra topología.

## Composición

`LivePursuit` reutiliza `SolarCaptureDebug`:

- carga, identidad y `bind` **sin debilitarlos**;
- un solo accessor nuevo de solo lectura: `bound_layer`.

El tramo se escribe en una **copia** de la capa `Body` enlazada y se dibuja con el mismo hook
(`World::render_solar_reveal_preview_with`):

- **Mismo dibujo que el cuerpo:** transform, shader, textura 1024 y **alfa del cuerpo**.
- **Orden del cuadro:** escena → ojos de las estrellas → overlays → percepción al final.
- **En reposo** (`None`): el cuadro es el de siempre.
- **Por cuadro:** no hay E/S, recarga ni reparseo; solo buffers reutilizados.

### Exportación de métricas

`shaft_min_px: null` significa medida no disponible: no hay tramo extendido o no hay centros de
anillo proyectados dentro de la pantalla. No significa grosor cero ni visibilidad confirmada.
Las medidas disponibles son números JSON finitos; no se emiten `inf`/`NaN` ni se relaja el parser.
Esto no cambia cámara, percepción, geometría ni PNG de la ruta observadora.

### Estado parcial de fix01

Se validan posiciones y normales derivadas antes de escribir cualquiera de los dos buffers; se
rechaza un lado anterior cuya norma desborde. El bind observador se prepara temporalmente y se
publica junto con el rig únicamente tras el éxito de todas las comprobaciones. Cada paso verifica
contra copias exactas de la geometría y partición Body aceptadas en bind, sin IO ni reparseo.
**LP-R01 y LP-R02 siguen abiertos:** el codo planar conserva las discontinuidades traseras y de
cambio de rama. No se presenta este candidato parcial como solución geométrica ni aprobación.

## Límites declarados

- **Prototipo de persecución:** no hay agarre ni arrastre, no se mueve la nave y el 0 % sigue indeciso.
- **Autocolisiones:** no se tratan las globales (distal contra cuerpo o tramo contra cuerpo).
- **Validez:**
  - no se afirma para distancias infinitas ni para todas las direcciones;
  - el cambio de lado del codo es un salto declarado;
  - la punta queda fuera de una bola de radio `a` alrededor del lazo.
- **Seguimiento del cuerpo:** con H ON (seguimiento horizontal 02), el cuerpo gira hacia la cámara y lo
  «trasero» del brazo casi no ocurre. Con H OFF se alcanza (pruebas reales a 180°).
- **Cámara de seguimiento:** cuando la punta llega a la nave, el distal ocupa la cámara (la nave está a
  22 u de ella). Es la cámara existente y no se cambió.
- **Grosor:** el tramo conserva el del lazo aprobado (≈ 3,9 u de radio frente a 80 del cuerpo). A gran
  distancia se lee fino.
- **Sin medir:** el rendimiento (no es prioridad). El teclado físico lo prueba Charlie.
- **Pruebas:**
  - las sintéticas están en `tests/tentacle_live_pursuit.rs`;
  - las reales son `#[ignore]` y necesitan `UMBRAL_CAPTURE_PACKAGE_DIR`, `UMBRAL_GROWTH_PACKAGE`,
    `UMBRAL_ARM_CORRESPONDENCE`, `UMBRAL_CAPTURE_CREATURE_OBJ` y `UMBRAL_CREATURE_TEXTURE`;
  - la secuencia es `examples/tentacle_live_pursuit_sequence.rs`.
