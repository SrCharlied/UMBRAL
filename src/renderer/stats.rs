use std::ops::AddAssign;

/// Contadores del renderizador, base de la evidencia de descarte y LOD (UMB-401/402/403).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Objetos que la escena presento al renderizador o descarto antes de dibujar.
    pub objects_submitted: usize,
    /// Objetos descartados por la esfera envolvente contra el frustum.
    pub objects_culled: usize,
    /// Vertices que pasaron por la etapa de vertices (MVP); el descarte de objetos los evita.
    pub vertices_processed: usize,
    pub triangles_submitted: usize,
    /// Triangulos totalmente fuera de un plano del frustum.
    pub triangles_culled: usize,
    /// Triangulos recortados contra el plano cercano.
    pub triangles_clipped: usize,
    /// Triangulos de espaldas descartados antes de rasterizar (solo materiales que lo piden).
    pub triangles_backface: usize,
    pub triangles_rasterized: usize,
    /// Fragmentos que pasaron la prueba de profundidad y ejecutaron el shader.
    pub fragments_evaluated: usize,
    pub pixels_written: usize,
}

impl AddAssign for RenderStats {
    fn add_assign(&mut self, o: Self) {
        self.objects_submitted += o.objects_submitted;
        self.objects_culled += o.objects_culled;
        self.vertices_processed += o.vertices_processed;
        self.triangles_submitted += o.triangles_submitted;
        self.triangles_culled += o.triangles_culled;
        self.triangles_clipped += o.triangles_clipped;
        self.triangles_backface += o.triangles_backface;
        self.triangles_rasterized += o.triangles_rasterized;
        self.fragments_evaluated += o.fragments_evaluated;
        self.pixels_written += o.pixels_written;
    }
}
