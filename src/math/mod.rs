//! Matematica del pipeline. Propietario: Persona 1 (ver `docs/architecture.md`).
//!
//! Convenciones (D3): sistema de mano derecha, Y arriba, la camara mira hacia -Z en el
//! espacio de vista, NDC en `[-1, 1]` (profundidad incluida), angulos en radianes y
//! matrices 4x4 de vectores columna (`m[fila][columna]`).

pub mod matrix;
pub mod vector;

pub use matrix::{Mat4, Transform};
pub use vector::{Vec2, Vec3, Vec4};
