# Criatura «La Vigilia» y paquetes del tentáculo

Assets de la criatura del Sol con ojo y de la captura automática, publicados por Charlie para todo el equipo.
Se versionan byte a byte (`assets/creature/** -text` en `.gitattributes`): los paquetes se verifican por hash al cargar.

| Ruta | Uso |
| --- | --- |
| `model/Meshy_AI_la_vigilia_remesh_30k_20261024_texture_remesh_tri.obj` + `.mtl` | Modelo de la criatura (Meshy AI, remesh 30k). El `.mtl` aporta solo colores; la textura viene del flag. |
| `texture_1024_linear_box_diagnostic.png` | Textura usada por el juego (`--creature-texture`). |
| `capture_v2/` | Paquete de captura del tentáculo (`--tentacle-capture-package`); `_superseded_v1/` se conserva tal cual. |
| `growth_package.txt` | Paquete de crecimiento (`--tentacle-growth-package`). |
| `arm_correspondence.txt` | Correspondencia del brazo (`--tentacle-arm-correspondence`). |

Juego completo con la criatura y la captura automática, desde la raíz del repositorio:

```bash
cargo run --release --locked -- --creature-solar-reveal --tentacle-auto-capture   --creature-model assets/creature/model/Meshy_AI_la_vigilia_remesh_30k_20261024_texture_remesh_tri.obj   --creature-texture assets/creature/texture_1024_linear_box_diagnostic.png   --tentacle-capture-package assets/creature/capture_v2   --tentacle-growth-package assets/creature/growth_package.txt   --tentacle-arm-correspondence assets/creature/arm_correspondence.txt
```

En ventana se suma la experiencia completa por defecto (estrellas, percepción, superficie lúcida, ojo del vacío y
tanque de 120 s). La captura automática requiere la percepción: con `--headless` o `--classic` añade `--lucid-perception`.
