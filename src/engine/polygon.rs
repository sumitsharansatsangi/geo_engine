use rkyv::Archived;

pub fn point_in_ring(lat: f32, lon: f32, ring: &Archived<Vec<(f32, f32)>>) -> bool {
    let n = ring.len();
    if n == 0 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;

    for i in 0..n {
        let (xi, yi): (f32, f32) = (ring[i].0.into(), ring[i].1.into());
        let (xj, yj): (f32, f32) = (ring[j].0.into(), ring[j].1.into());
        let denom = yj - yi;
        if denom != 0.0 && ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / denom + xi)
        {
            inside = !inside;
        }
        j = i;
    }

    inside
}
