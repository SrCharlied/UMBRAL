//! UMBRAL: Asfixia Lucida. Propietarios y contratos en `docs/architecture.md`.

// El modulo aprobado de la percepcion (UMB-310, variante 02) se compila sin cambios tambien
// fuera de la biblioteca (`#[path]` en sus pruebas y ejemplo), donde nombra el crate `umbral`.
extern crate self as umbral;

pub mod app;
pub mod assets;
pub mod config;
pub mod debug;
pub mod math;
pub mod renderer;
pub mod scene;
pub mod visual;
