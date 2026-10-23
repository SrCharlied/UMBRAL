//! Lectura de recursos. `obj.rs`: Persona 1; `assets/models`: Persona 3.

use std::path::{Path, PathBuf};

pub mod obj;

/// OBJ de la nave dentro del checkout donde se compilo el crate.
///
/// Limitacion heredada: usa `CARGO_MANIFEST_DIR`, por lo que el binario no es
/// reubicable sin recompilar.
pub fn ship_model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models/ship/Umbral_v4_Desgastada.obj")
}
