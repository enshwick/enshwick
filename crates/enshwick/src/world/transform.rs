//! Where a thing sits in the world and how it is turned.

/// A position in metres and a rotation, as the game places things.
///
/// # Examples
///
/// ```
/// use enshwick::world::Transform;
///
/// let at = Transform { position: [100.0, 40.0, 100.0], rotation: [0.0, 0.0, 0.0, 1.0] };
/// assert_eq!(at.rotation[3], 1.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Transform {
    /// Where, in metres.
    pub position: [f64; 3],
    /// The rotation, a quaternion in x, y, z, w order.
    pub rotation: [f32; 4],
}
