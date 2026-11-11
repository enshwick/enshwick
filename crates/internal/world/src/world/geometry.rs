//! Distance tests between places in the world.

/// Whether `at` lies within `radius` metres of `center` across the ground
/// and within `height` metres above or below it.
///
/// # Examples
///
/// ```
/// use enshwick::world::near;
///
/// assert!(near([0.0, 0.0, 0.0], [6.0, 3.0, 8.0], 10.0, 4.0));
/// assert!(!near([0.0, 0.0, 0.0], [6.0, 5.0, 8.0], 10.0, 4.0));
/// assert!(!near([0.0, 0.0, 0.0], [9.0, 0.0, 9.0], 12.0, 4.0));
/// ```
#[must_use]
pub fn near(center: [f64; 3], at: [f64; 3], radius: f64, height: f64) -> bool {
    let (dx, dy, dz) = (at[0] - center[0], at[1] - center[1], at[2] - center[2]);
    dx.mul_add(dx, dz * dz) <= radius * radius && dy.abs() <= height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_measures_ground_and_height_apart() {
        let c = [100.0, 50.0, -20.0];
        assert!(near(c, c, 0.0, 0.0));
        assert!(near(c, [112.0, 50.0, -20.0], 12.0, 4.0));
        assert!(!near(c, [112.1, 50.0, -20.0], 12.0, 4.0));
        assert!(near(c, [100.0, 46.0, -20.0], 12.0, 4.0));
        assert!(!near(c, [100.0, 54.5, -20.0], 12.0, 4.0));
        assert!(!near(c, [109.0, 50.0, -11.0], 12.0, 4.0));
    }
}
