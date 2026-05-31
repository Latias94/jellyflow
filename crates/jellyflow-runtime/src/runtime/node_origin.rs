pub(crate) fn normalize_node_origin(origin: (f32, f32)) -> (f32, f32) {
    let mut ox = origin.0;
    let mut oy = origin.1;
    if !ox.is_finite() {
        ox = 0.0;
    }
    if !oy.is_finite() {
        oy = 0.0;
    }
    (ox.clamp(0.0, 1.0), oy.clamp(0.0, 1.0))
}
