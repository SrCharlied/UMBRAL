# Percepción alterada por oxígeno, variante 02 (UMB-310), preview aislado

**Estado:** una sola hipótesis, **pendiente de decisión artística de Charlie**.
- Responde a su dirección explícita: tono gris + morado del monstruo, pérdida de la visión periférica hasta ver casi solo la nave, visión borrosa, y negro al 0 % o al ser atacado.
- Sustituye en arte a la 01, que era demasiado sutil. La 01 (`lucid_perception`, sus pruebas, su ejemplo y su doc) sigue intacta.
- No está exportada en `visual/mod.rs` ni integrada a App, World o CLI. El módulo solo lo compilan, mediante `#[path]`, `tests/lucid_perception_variant.rs` y `examples/lucid_perception_variant_preview.rs`.
- **No incluye** combate, daño, colisiones, duración ni recuperación de un ataque, consumo de O₂, estrellas observadoras (futuro, umbral sin decidir), HUD ni audio.

## Interfaz

```rust
pub fn lucid_perception_variant(source: &Framebuffer, oxygen01: f64, elapsed_time: f64,
    focus_uv: [f64; 2], attack_blackout: bool) -> Result<Vec<u32>, String>
```

- `focus_uv` es la posición de la nave en pantalla, en `[0, 1]²`. En el ejemplo sale de proyectar `World.ship.position` con la cámara real (en el arranque `--near`, unos 640 × 455 px; no el centro geométrico).
- La función solo lee `source` y devuelve un `Vec` RGB nuevo. No toca píxeles, dimensiones, profundidad, cámara, materiales, texturas ni geometría.

## Qué hace

Con O₂ ≥ 0.5 y sin ataque devuelve una **copia exacta**. Por debajo, una misma curva `s(O₂) = smoothstep(0.5 → 0.05)` controla tres mecanismos separados:

1. **Gris + morado:** `color' = mezcla(color, luma · tinte, 0.9 · s)`.
   - El tinte combina el gris (125, 117, 119) y el morado medio (65, 20, 67) del monstruo, cada uno normalizado por su luminancia, con un peso de 0.6 para el morado.
   - La mezcla conserva la luminancia calculada antes de cuantizar y limitar los canales RGB a 8 bits; las luces altas pueden perder luminancia al saturarse R y B. El negro sigue negro y el espacio no se rellena de morado.
2. **Túnel hacia la nave:** el negro avanza desde la periferia según la distancia al foco, medida en unidades del lado menor.
   - **Abierto** (radio 2.2 + 0.5 de borde): más allá de las esquinas de los encuadres 16:9 entregados. Ese alcance no es una garantía para proporciones arbitrariamente anchas o altas, pues se mide respecto al lado menor.
   - **Cerrado** en el tramo crítico (0.19 + 0.11): radio de unos 137 px sin atenuación del túnel y negro a partir de unos 216 px a 720p. Esa zona también recibe tinte y desenfoque; no es una región nítida ni píxel-exacta. La nave ocupa unos 120 px alrededor de su punto proyectado.
3. **Visión borrosa:** desenfoque gaussiano separable real, filtrando vecinos de `source` con bordes acotados.
   - Sigma de hasta el 0.5 % del lado menor: 3.6 px a 720p.
   - La nave queda borrosa pero reconocible.
4. **Negro final continuo:** entre el 1 % y el 0 % todo funde a negro.
   - Al 1 % la nave se ve completa, igual que al 5 %.
   - Con O₂ 0, o con `attack_blackout = true` y cualquier O₂, todos los píxeles son 0.
   - En el fundido final se conserva un mínimo de un nivel RGB si una muestra con señal positiva dentro del túnel se redondearía a negro. Así la cuantización no adelanta el blackout total; las muestras negras y el exterior del túnel permanecen negros. Este límite de RGB8 no garantiza que una señal de un nivel sea perceptible en cualquier monitor.

El efecto es **estático en el tiempo**: sin pulsos, onda ni jitter. El tiempo se valida, pero esta hipótesis no lo usa.

### Parámetros (candidatos, NO aprobados)

| Constante | Valor |
|---|---|
| `PERCEPTION_ONSET_OXYGEN` / `PERCEPTION_FULL_OXYGEN` / `BLACKOUT_FADE_OXYGEN` | 0.5 / 0.05 / 0.01 |
| `MONSTER_PURPLE` / `MONSTER_GREY` | (65, 20, 67) / (125, 117, 119), solo valores numéricos de referencia de tono |
| `TINT_PURPLE_SHARE` / `TINT_MAX` | 0.6 / 0.9 |
| `TUNNEL_OPEN_RADIUS` / `TUNNEL_OPEN_SOFTNESS` | 2.2 / 0.5 |
| `TUNNEL_CLOSED_RADIUS` / `TUNNEL_CLOSED_SOFTNESS` | 0.19 / 0.11 |
| `BLUR_MAX_SIGMA_FRACTION` | 0.005 |

## Validación (antes de reservar, también en los atajos de identidad y de negro)

`Err` claro, sin sanear a un valor válido, si:
- el **O₂** está fuera de `[0, 1]` o no es finito;
- el **tiempo** es negativo o no finito;
- el **foco** está fuera de `[0, 1]²` o no es finito;
- el **`Framebuffer`** tiene dimensiones nulas, `width × height` desborda o supera `MAX_PIXELS`, o `pixels.len()` no cuadra.

## Preview

```
cargo run --release --locked --offline --example lucid_perception_variant_preview -- --output-dir <carpeta-nueva> [--time <s>]
```

**Escena:** es la misma base que la 01.
- Usa `World` real con la nave versionada y los setters aprobados (superficie lúcida en Antena, ojo del vacío en Vigilia), con el arranque `--near` junto a cada uno.
- O₂ del mundo 1 y tiempo de mundo **fijos por ejecución** (1.3 s por defecto; `--time` cambia ese valor y el inicio temporal del postproceso). Cada escena se carga y se dibuja una vez; la comparación con el juego debe usar el mismo tiempo.

**Salida** (1280×720, sin rótulos):
- el baseline;
- la referencia 01 y la variante 02 al 100, 50, 25, 10, 5, 1 y 0 % de O₂ del postproceso;
- la muestra de ataque con O₂ 100 %;
- `sequence/`: una caída **guionizada** de O₂ (del 50 % a 0 en 40 cuadros, y luego negro sostenido; 12 fps). No es consumo real.

**`manifest.json`** (std, JSON válido) separa el O₂ y el tiempo del mundo del O₂, el tiempo y el ataque del postproceso. Incluye el foco proyectado, los parámetros y métricas por imagen: fracción negra, luma media, violeta y zona de la nave visible.

**Rechazos:** argumentos inválidos, el tiempo, el foco y todos los casos se validan antes de crear la carpeta, que debe ser nueva.

## Límites conocidos

- Al 25 % el túnel apenas oscurece las esquinas; lo que se nota es el tinte y el desenfoque. El cierre fuerte ocurre entre el 25 % y el 10 %.
- El 5 % y el 1 % son idénticos a propósito: la curva ya está al máximo y el negro final empieza por debajo del 1 %.
- En las luces altas el tinte satura R y B, y se lee más rosa-magenta que morado oscuro.
