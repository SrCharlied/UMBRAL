//! Parametros centralizados. Ver `docs/architecture.md`.
//!
//! La resolucion define el framebuffer (render); la ventana se abre con las mismas
//! dimensiones (presentacion). Los limites los valida `Framebuffer::new`.

/// 1280x720 (16:9), decision aprobada por Charlie. El baseline heredado usa 1280x960.
pub const DEFAULT_WIDTH: usize = 1280;
pub const DEFAULT_HEIGHT: usize = 720;
