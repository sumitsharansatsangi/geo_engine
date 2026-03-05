use rkyv::Archived;

pub fn point_in_ring(lat: f32, lon: f32, ring: &Archived<Vec<(f32, f32)>>) -> bool {
    if ring.is_empty() {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;

    for i in 0..ring.len() {
        let pi = &ring[i];
        let pj = &ring[j];
        let xi: f32 = pi.0.into();
        let yi: f32 = pi.1.into();
        let xj: f32 = pj.0.into();
        let yj: f32 = pj.1.into();

        let intersect =
            ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi);

        if intersect {
            inside = !inside;
        }

        j = i;
    }

    inside
}
