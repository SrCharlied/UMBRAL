use crate::{
    math::{Mat4, Vec3},
    scene::solar_system::SolarSystem,
};

/// Ancla local al planeta: sigue tanto su orbita como su rotacion axial.
#[derive(Clone, Debug)]
pub struct Collectible {
    pub id: usize,
    pub planet_index: usize,
    pub local_offset: Vec3,
    pub collected: bool,
}
impl Collectible {
    pub fn position(&self, system: &SolarSystem, time: f64) -> Option<Vec3> {
        if !time.is_finite() || !self.local_offset.is_finite() {
            return None;
        }
        let body = system.bodies.get(self.planet_index)?;
        let position = body.position(time)
            + Mat4::rotation_y(body.rotation_angle(time)).transform_dir(self.local_offset);
        position.is_finite().then_some(position)
    }
}

#[derive(Clone, Debug)]
pub struct NarrativeLog {
    pub anchor: Collectible,
    pub title: &'static str,
    pub text: &'static str,
}

pub(crate) fn catalog() -> (Vec<Collectible>, Vec<NarrativeLog>) {
    let anchor = |id, y| Collectible {
        id,
        planet_index: id + 1,
        local_offset: Vec3::new(0.0, y, 0.0),
        collected: false,
    };
    // Balizas polares fuera de la superficie; los offsets son unidades de mundo.
    let resources = [26.0, 21.0, 32.0]
        .into_iter()
        .enumerate()
        .map(|(i, y)| anchor(i, y))
        .collect();
    let texts = [
        (
            "Antena: la respuesta",
            "La antena llevaba anos sin energia. Hoy respondio a nuestro saludo antes de que lo enviaramos. La voz al otro lado era la mia, pero seguia respirando cuando yo contuve el aire.",
        ),
        (
            "Osario: inventario",
            "Catalogamos estas curvas como restos minerales. El analisis encontro calcio y marcas de herramientas. No son huesos de una criatura: son las costillas de la nave que aun estamos construyendo.",
        ),
        (
            "Vigilia: el umbral",
            "El oxigeno no nos protege del vacio; nos impide verlo. Las tres balizas apuntan al Sol. Cuando recuperes los registros, acercate a su luz. No mires atras si las estrellas cierran los ojos.",
        ),
    ];
    let logs = [26.0, 21.0, 32.0]
        .into_iter()
        .enumerate()
        .map(|(i, y)| {
            let mut a = anchor(i, y);
            a.local_offset.x = 14.0;
            NarrativeLog {
                anchor: a,
                title: texts[i].0,
                text: texts[i].1,
            }
        })
        .collect();
    (resources, logs)
}
