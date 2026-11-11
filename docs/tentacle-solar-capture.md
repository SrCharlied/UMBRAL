# Composición observadora del tentáculo en el Sol con ojo (UMB-311, debug opt-in)

El brazo aprobado (tramo de crecimiento, ley `fixed`) entra en el juego completo **solar**, en la
**misma capa del cuerpo** de la criatura.

**Es una prueba visual manual (F7), NO una captura jugable:**

- no hay agarre funcional, persecución ni orientación 6DoF;
- no hay bloqueo de input, audio, apagón, explosión ni reglas de ataque;
- sin el flag, el juego no cambia.

**Código:** `src/visual/tentacle_solar_game.rs`. Copias **exactas** de los módulos aceptados:
`src/visual/tentacle_growth_cpu.rs` y `src/visual/tentacle_capture_cpu.rs`. El primero incluye al segundo
como submódulo `capture`.

## Uso

```text
umbral --creature-model <Meshy.obj> --creature-texture <texture_1024_linear_box_diagnostic.png> \
  --creature-solar-reveal [--lucid-surface --void-eye --lucid-perception --watching-stars] \
  --tentacle-capture-debug --tentacle-capture-package <capture_v2> \
  --tentacle-growth-package <growth_package.txt> --tentacle-arm-correspondence <arm_correspondence.txt> \
  [--tentacle-capture-time S]
```

- **Validación previa a cualquier IO:** con el flag faltan rutas o el Sol → error literal; con
  `--legacy` → error; las opciones sin el flag → error.
- **Rutas solares admitidas:** las mismas de siempre (con o sin superficie lúcida, ojo del vacío,
  percepción y estrellas), con sus guardas.
- **F7** (solo en este modo):
  - toma la **posición** de la nave y el transform **actual** de la criatura;
  - reproduce y **sostiene** en el último cuadro;
  - otra F7 cancela y vuelve la criatura completa. Es un salto de debug declarado, no una coreografía.
- **Rechazos de dominio** (contacto fuera del cono, antiparalelo, no finito): se escriben en consola y en
  el título, sin cambiar nada.
- **`--tentacle-capture-time S`:** activa al arrancar con el reloj en `S`. Es el **mismo** camino que F7;
  sirve para headless/replay determinista.
- **O₂:** el existente (1–4, M/N). Con O₂ ≥ 50 % la criatura **y el brazo** siguen invisibles por
  contrato; la composición no baja el O₂ ni mueve nave o cámara para mostrarse.
- **Cámara y vistas:** las del juego solar (7/8, R, H, G). El estado va en el título; no hay HUD nuevo.

## Datos (privados, por argumentos; nunca embebidos)

- **Captura aprobada:** `capture_package.txt` + OBJ, verificados por tamaño y FNV.
- **Paquete del tramo aprobado:** `growth_package.txt`, ligado por FNV a esa captura. Se reutilizan, sin
  cambios, `GrowthPackage`, `GrowthData::offset` y `GrowthData::evaluate_with`.
- **Correspondencia brazo → Meshy:** `arm_correspondence.txt`, texto estricto exportado fuera del
  repositorio. Contiene:
  - FNV de la captura y del paquete;
  - huella `geometry_fnv` del Meshy;
  - 1422 vértices y 2827 caras del brazo.

## Identidad (`bind`, una vez tras `SolarGame::start`)

Se verifica contra la criatura **real** del World:

- **Modelo:**
  - huella de geometría y conteos del Meshy;
  - bounds = los ORIGINALES de la captura.
- **Brazo:**
  - cada cara del brazo es la cara Meshy indicada con el mismo winding y material;
  - posiciones a ≤ 1,5e-6 por componente. El export OBJ de 6 decimales da 1,0e-6 medido: **no es bit a
    bit**;
  - quitar el brazo de la partición `body` quita exactamente sus caras.
- **Ojo:** no usa ningún vértice del brazo.
- **Raíz:** la de la captura (27) es la **única** compartición con el cuerpo.

La criatura del World, su partición, el ojo (`EyeRig`) y el seguimiento **no cambian**. No hay
`set_creature`/`clear_creature`, ni nuevos bounds o placement.

## Composición

Con la composición activa, el compositor solar recibe **una** capa `Body` alternativa
(`SolarBodyOverride`):

- **Malla:**
  - los vértices, UV y normales de la partición `body` como prefijo exacto;
  - sus caras sin las del brazo;
  - el tramo evaluado, en espacio OBJ.
- **Mismo dibujo que el cuerpo:** transform (el girado por el seguimiento 02, raíz continua), material,
  shader, textura y **alfa del cuerpo**.
- **Sin cambios:** Sol, ojo y `SolarPairCompositor`.
- **Descarte:** suma la esfera del tramo (bounds de dibujo aparte) a la esfera original.
- **Validación:** `World::render_solar_reveal_preview_with` valida todo **antes** de limpiar el cuadro
  (prefijo, índices, UV/normales, finitud, esfera, shader). Un `Err` deja color, profundidad y World
  intactos.
- **Orden del cuadro:** escena (con la capa) → ojos de las estrellas (se ocluyen con el brazo) →
  overlays → percepción, al final.
- **APIs viejas:** `render_solar_reveal_preview`, `SolarGame::render` y `render_game_frame` delegan con
  `None`, con el cuadro y las estadísticas exactos.

## Tiempo

- **Reloj propio:** avanza con segundos **reales**. No toca el reloj del mundo y F5 no lo congela.
- **Retiming PROVISIONAL de preview:** `sqrt(max(1, d/d0))` en alcance y arrastre.
- **Cuadro base:** piso del tiempo base × 24 + 1, con hold en el último.
- **Fidelidad:** la aritmética es la de `GrowthPlan::sample`; hay una prueba de equivalencia bit a bit.

## Límites

- **Observadora:**
  - si la nave o el cuerpo se mueven, el objetivo **no** los sigue (instantánea);
  - la espiral cierra donde estaba la nave, con la orientación horneada;
  - la «deriva del cuerpo» del título no es un error del agarre.
- **Lo que no hay:** 6DoF, persecución, mezcla con el idle, regla de juego, valores de distancia, O₂ o
  cooldown.
- **Raíz:** la del tramo procede del export de la captura (≤ 1e-6 OBJ del cuerpo Meshy). Es una costura
  medida, no exacta.
- **Pruebas sintéticas:** `tests/tentacle_solar_game.rs`. Las de assets reales y el oráculo de 936
  cuadros están en `#[ignore]` y necesitan:
  - `UMBRAL_CAPTURE_PACKAGE_DIR`, `UMBRAL_GROWTH_PACKAGE`, `UMBRAL_ARM_CORRESPONDENCE`;
  - `UMBRAL_CAPTURE_CREATURE_OBJ`, `UMBRAL_CREATURE_TEXTURE`;
  - `UMBRAL_GROWTH_ORACLE_DIR`.
- **Rendimiento:** no se midió.

## Corrección FIX01 (sin promoción a Root)

- **Salida y entradas:** antes de cargar/renderizar, la ruta de captura (CLI y API pública)
  protege `capture_package.txt`, **todos** sus OBJ referenciados, `growth_package.txt`, la
  correspondencia y el modelo/textura de la criatura. Se comprueban nombres canónicos e identidad
  de archivo; un hardlink con otro nombre también se rechaza. La misma comprobación se repite
  inmediatamente antes de cada guardado headless o `P`.
- **Identidad nativa:** en Windows se usa `GetFileInformationByHandleEx(FileIdInfo = 0x12)`;
  `FILE_ID_INFO` combina volumen `u64` e ID de 128 bits, no una huella de contenido. El FFI está
  encapsulado en `app::capture_output`, con errores propagados. Los handles de lectura se conservan
  durante la ejecución, con `FILE_SHARE_READ` (sin compartir escritura/borrado). La std estable
  utilizada aún rechaza `MetadataExt::volume_serial_number/file_index` con E0658; no se añadieron
  dependencias ni se modificó Cargo. En Unix se usa `MetadataExt` (dev/inode); esa rama no fue
  ejecutada en esta entrega Windows. No se promete un sandbox contra junctions o carreras
  adversariales de directorios; el preflight y los handles son defensa preventiva de estos archivos.
- **`Some` es un contrato explícito:** exige preview solar, etapas, criatura y prefijo válido
  incluso con ojo oculto. La esfera externa debe contener todos los vértices del apéndice.
  Se comprueban matriz **compuesta** MVP y posiciones/atributos transformados usados por la
  etapa de vértices antes de limpiar. No se toca cámara/math ni se cambia el fallback `None`.
- **Publicación del cuadro:** `update_frame` resuelve World/deriva primero y entrega a
  `evaluate_with` una malla temporal (el core puede escribir parcialmente al fallar). Growth,
  cuerpo compuesto, esfera y status se publican solo tras todas las comprobaciones.
- **Conteos y tiempo:** el parser compara el conteo recibido sin sumar uno a un `usize` externo
  ni reservar memoria proporcional a él. `CaptureTimeline::new` exige distancia no negativa,
  tiempos base finitos y ventanas positivas, ordenadas y dentro de duración. La fórmula para
  entradas válidas y la guarda de overflow del reloj se conservan.
- **Límite de identidad de la correspondencia:** los campos `*_sha256` solo validan **sintaxis**
  hexadecimal; no autentican los archivos ni establecen identidad criptográfica. FNV tampoco es
  autenticación. Prefijo UV/normales, índices y finitud se validan; no se afirma equivalencia
  semántica adicional de UV/normales entre exports ni se retoca su arte.

Referencias ABI consultadas: Microsoft Learn, `GetFileInformationByHandleEx`, `FILE_ID_INFO` y
`FILE_INFO_BY_HANDLE_CLASS` (winbase.h/minwinbase.h). La evidencia de la API, RED y ejecución está
fuera de `source`, en `fix01/`; F7/P físicos quedan para Charlie.

## Corrección FIX02: grafo efectivo de entradas (SC01-MTL, sin promoción)

FIX01 omitía los MTL que `load_model` realmente abre y el modelo builtin de la nave.
La regresión nativa previa escribió un PNG sobre un MTL **copiado** a través de un hardlink;
los controles válidos CLI/API excluyen el falso positivo de un fixture con MTL ausente.

- El guard de captura ahora fija el **OBJ de `ship_model_path()` y sus `material_libraries`**,
  el **OBJ de la criatura y sus `material_libraries`**, la textura explícita, el manifest de
  captura y todos sus payloads OBJ, el paquete de crecimiento y la correspondencia.
- Cada OBJ queda abierto con identidad/`FILE_SHARE_READ` antes de descubrir sus dependencias;
  se usa `parse_obj` congelado, no un scanner aproximado de líneas. Para cada MTL se aplica
  exactamente el guard de componentes `Normal` y la resolución relativa del loader.
  Se admiten múltiples declaraciones, subcarpetas y comentarios como el parser actual.
- No se recorre el árbol de assets ni se descubren `map_Kd` u otras texturas MTL: el loader
  actual **no las lee**. Los OBJ del paquete usan `parse_obj`, no `load_model`, y no abren MTL.
- El grafo se clasifica **una vez al arrancar**, sin IO ni recarga de modelos por cuadro.
  Se conservan handles RAII, FileID Windows, preflight de arranque y comprobación repetida
  justo antes de headless/`P`. Un nombre igual o un hardlink de cualquier entrada se rechaza
  antes del writer; guardar de nuevo un PNG válido no alias sigue permitido.
- Los tests privados inspeccionan el grafo builtin mediante lecturas/preflight solamente.
  Las pruebas destructivas y de sharing usan copias desechables; ninguna apunta un writer
  a los inputs originales, Root o FIX01. No se expone ninguna API unsafe para pruebas.
- Windows se ejecutó nativamente; Unix **no se ejecutó**. No se certifican sandbox, carreras
  adversariales de directorios ni interacción física F7/P. La evidencia RED/GREEN, comandos,
  oráculos y hashes de FIX02 está fuera de `source`, en `fix02/`.
- Esta corrección no cambia geometría, shaders, cámara, ley, física, O₂ ni defaults del juego.
  El launcher de prueba es aislado; el futuro Root deberá arrancar O₂=1, inactivo, F7 manual.
