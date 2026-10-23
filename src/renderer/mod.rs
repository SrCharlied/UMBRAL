//! Pipeline de software. Propietario: Persona 1 (ver `docs/architecture.md`).
//!
//! `framebuffer`, `rasterizer` e `image` son la base reutilizable; `legacy` es el
//! visor heredado y se reemplazara en UMB-101/UMB-102.

pub mod framebuffer;
pub mod image;
pub mod legacy;
pub mod rasterizer;
