# UMB-310 preview02: «pupila reforzada progresiva» (preview AISLADO)

Variante de una sola hipótesis sobre preview01 («Pupilas de alfiler», ver
`docs/watching-stars-preview.md`). Charlie: «es solo con la percepción que se deberían notar más,
tal vez si mejorar cómo se ve la pupila». No integra nada: sin cambios en exports, App, World,
renderer, Cargo, CLI ni en la percepción aprobada (`lucid_perception_variant.rs`, SHA
`76e149ba…`). Composición intacta: ojos sobre el cuadro → percepción aprobada (nunca ojos encima).

## Qué cambia

Solo `src/visual/watching_stars.rs`:

- `pupil_reinforcement(o2)`: 0 con O2 ≥ 50 % (`PUPIL_REINFORCE_ONSET_OXYGEN`, el mismo arranque
  que la percepción), smoothstep hasta 1 en el 25 % (`PUPIL_REINFORCE_FULL_OXYGEN`), constante
  por debajo. Valida el O2 igual que `observer_weights`. En la matriz: 40 % → 0.352; 25/10/0 % → 1.
- En `draw_eye`, cada submuestra mezcla el color de preview01 con el del ojo reforzado según ese
  peso. Ojo reforzado (peso 1): pupila de 1.8 px de radio (antes 0.8), iris simplificado a un
  borde de 1.8 a 2.3 px con el ámbar de preview01 a la mitad de brillo (`REINFORCED_IRIS_GAIN`),
  esclerótica sin cambios. Mismo centro de pupila (offset hacia la nave, hasta 1.3 px).
- No cambian: almendra 9x5, apertura, cobertura y opacidad (alfa), población, curva, ranking e IDs,
  offsets de mirada, profundidad, oclusión ni el informe (salvo `pixels_changed`). Con peso 0 se
  ejecuta exactamente el camino de preview01. Ningún canal de ningún píxel se aclara.

`examples/watching_stars_preview.rs` cambia solo metadatos: el nombre de la hipótesis y el bloque
`policy.pupil_reinforcement` de `matrix.json`. Cámaras, resolución, matriz de 27 estados, guion de
109 cuadros y archivos son los mismos.

## Pruebas (`tests/watching_stars_pupil.rs`)

El prefijo es el tracer del padre (1505 bytes, intacto). A continuación:

- Con RED observado: curva del refuerzo; oscuridad añadida dentro del ojo y hacia la nave (antes
  del post); luz del ojo desplazada en sentido opuesto a la nave DESPUÉS de la percepción aprobada
  (vistas normal y cielo al 25 %).
- Guards nacidos verdes (prefijo `born_green_`), con sensibilidad demostrada por mutación en una
  copia:
  - O2 ≥ 50 % exacto a preview01: píxeles, informe y combined en normal, `--near 3` y cielo, más
    un barrido del 50 al 70 %;
  - población, mirada y metadatos de IDs;
  - soporte exterior 9x5 sin cambios y nada pintado sobre geometría;
  - esclerótica y opacidad bit a bit iguales fuera del núcleo oscuro;
  - nunca aclara un canal;
  - preflight igual que preview01;
  - combined negro al 0 %.

`tests/fixtures/watching_stars_v1.rs` es la copia padre inmutable de preview01 que sirve de
referencia; `tests/watching_stars.rs` (25 tests de preview01) no se tocó.

## Reproducir

```
cargo test --locked --offline --test watching_stars_pupil
cargo run --release --locked --offline --example watching_stars_preview -- --output-dir <carpeta-nueva>
```

## Resultado y límite

La medición en el juego real a 1280x720 (mediana por ojo aislado, en combined) da:

| O2 | Centroide de luz a lo largo de la mirada | Hueco bajo la pupila | Pico de luma |
|---|---|---|---|
| 40 % | −0.16 → −0.33 px | 0.77 → 0.83 | 177 → 173 |
| 25 % | −0.17 → −0.88 px | 0.11 → 0.38 | 91 → 61 |

Es un sesgo medible, no la prueba de que una persona reconozca un ojo. A escala nativa, al 25 %
tras el desenfoque (σ ≈ 2.1 px), los ojos siguen viéndose como manchas rosas redondas: la pupila no
se lee como pupila, y el ojo pierde ~33 % de pico, así que se nota MENOS como mancha. Al 40 % ayuda
un poco. Diagnóstico: dentro de 9x5 la masa oscura no puede ser bastante ancha frente al
desenfoque sin apagar el ojo. La hipótesis queda preservada aquí y no se amplía (ni tamaño, ni
brillo, ni post). La evidencia completa está en `../worker-evidence/README.md`.
