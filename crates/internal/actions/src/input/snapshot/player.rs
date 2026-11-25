//! The local player's own parts of a [`ClientSnapshot`]: where it stands
//! and looks, its health and stamina, and its action bar's selected slot.

use super::{ClientSnapshot, Meter, SnapshotPart};

impl ClientSnapshot {
    /// Where the local player stands, x, y and z in metres, as
    /// [`World::position`](crate::world::World::position) gives a position.
    ///
    /// The value is the client's own copy of the world, so on a joined
    /// client it may trail the server by a step. The local player's parts
    /// read in its last step, so a menu that stops the steps leaves them
    /// unread after a second.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn near(snapshot: &ClientSnapshot, at: [f64; 3]) -> bool {
    ///     snapshot.position().is_some_and(|p| {
    ///         p.iter().zip(at).map(|(a, b)| (a - b).powi(2)).sum::<f64>() < 25.0
    ///     })
    /// }
    /// ```
    #[must_use]
    pub fn position(&self) -> Option<[f64; 3]> {
        self.reads(SnapshotPart::Position)
            .then_some(self.raw.position)
    }

    /// Where the local player's camera looks: a unit quaternion x, y, z, w
    /// in the world's axes, the camera the client reports to the server.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn looks_somewhere(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.look().is_some()
    /// }
    /// ```
    #[must_use]
    pub fn look(&self) -> Option<[f32; 4]> {
        self.reads(SnapshotPart::Look).then_some(self.raw.look)
    }

    /// The local player's health and its maximum, as the HUD draws them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn hurt(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.health().is_some_and(|health| health.fraction() < 0.3)
    /// }
    /// ```
    #[must_use]
    pub fn health(&self) -> Option<Meter> {
        self.vitals(self.raw.health, self.raw.health_max)
    }

    /// The local player's stamina and its maximum, as the HUD draws them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn winded(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.stamina().is_some_and(|stamina| stamina.current == 0)
    /// }
    /// ```
    #[must_use]
    pub fn stamina(&self) -> Option<Meter> {
        self.vitals(self.raw.stamina, self.raw.stamina_max)
    }

    /// A meter of the vitals group, while it reads.
    fn vitals(&self, current: u32, max: u32) -> Option<Meter> {
        let value = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
        self.reads(SnapshotPart::Vitals).then(|| Meter {
            current: value(current),
            max: value(max),
        })
    }

    /// The action bar's selected slot, from 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn first_selected(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.actionbar_slot() == Some(0)
    /// }
    /// ```
    #[must_use]
    pub fn actionbar_slot(&self) -> Option<u8> {
        if !self.reads(SnapshotPart::Actionbar) {
            return None;
        }
        u8::try_from(self.raw.actionbar_slot).ok()
    }
}
