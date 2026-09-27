#[inline]
pub fn vec3_length(v: [f32; 3]) -> f32 {
    let [x, y, z] = v;
    libm::sqrtf(z * z + x * x + y * y)
}

/// `Vec3Distance`: how far `a` is from `b`.
#[inline]
pub fn vec3_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    vec3_length([a[0] - b[0], a[1] - b[1], a[2] - b[2]])
}

/// How far `a` is from `b` in the ground plane, height ignored.
#[inline]
pub fn vec3_distance_2d(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y) = (a[0] - b[0], a[1] - b[1]);
    libm::sqrtf(x * x + y * y)
}
