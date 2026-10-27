//! What the player has uncovered of the map, on the client.

use crate::abi::{FogInfo, Header, Status};
use crate::{Host, Load, Off};

/// The client's fog of war: how much of each part of the map the player
/// has uncovered, the game's `FogOfWar`. Only the player's own machine
/// has it, from the save of the world they play.
///
/// The loader copies the fog as the game uncovers it, so a mod reads it
/// from any thread. Pixels are square, one byte each, from 0 (hidden) to
/// 255 (uncovered); row 0 is the world's high z edge, and x grows with
/// world x.
///
/// # Examples
///
/// ```
/// use core::ops::ControlFlow;
/// use std::time::Duration;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let fog = load.fog_of_war()?;
///     load.scheduler()?.every(Duration::from_secs(5), move || {
///         if let Ok(Some(seen)) = fog.coverage([1_200.0, 40.0, 3_400.0]) {
///             enshwick::debug!("coverage {seen}");
///         }
///         ControlFlow::Continue(())
///     });
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct FogOfWar {
    host: Host,
}

/// The fog's size, scale and how often it changed, as one read found it.
///
/// # Examples
///
/// ```
/// use enshwick::map::FogState;
///
/// fn corner(state: FogState) -> Option<[u32; 2]> {
///     state.pixel_of([0.0, 0.0, state.size_z - 1.0])
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogState {
    /// The image's width in pixels.
    pub width: u32,
    /// The image's height in pixels.
    pub height: u32,
    /// The metres one pixel covers along each axis.
    pub metres_per_pixel: f64,
    /// The world's z size the fog covers, in metres.
    pub size_z: f64,
    /// Grows whenever pixels changed: a mod that compares it reads again
    /// only when it moved.
    pub version: u64,
    /// Changes with each world the client enters; the copy of a world is
    /// gone once the client leaves it.
    pub session: u64,
}

impl FogState {
    fn from_abi(info: &FogInfo) -> Self {
        Self {
            width: info.width,
            height: info.height,
            metres_per_pixel: f64::from(info.metres_per_pixel),
            size_z: f64::from(info.size_z),
            version: info.version,
            session: info.session,
        }
    }

    /// The pixel that holds `position`, in metres as
    /// [`World::position`](crate::world::World::position) gives it; `None`
    /// outside the fog.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::map::FogState;
    ///
    /// let state = FogState {
    ///     width: 1280,
    ///     height: 1280,
    ///     metres_per_pixel: 8.0,
    ///     size_z: 10_240.0,
    ///     version: 0,
    ///     session: 1,
    /// };
    /// assert_eq!(state.pixel_of([12.0, 50.0, 10_230.0]), Some([1, 1]));
    /// assert_eq!(state.pixel_of([-1.0, 0.0, 0.0]), None);
    /// ```
    #[must_use]
    pub fn pixel_of(&self, position: [f64; 3]) -> Option<[u32; 2]> {
        if self.metres_per_pixel <= 0.0 {
            return None;
        }
        let x = (position[0] / self.metres_per_pixel).floor();
        let y = ((self.size_z - position[2]) / self.metres_per_pixel).floor();
        let inside = |value: f64, limit: u32| value >= 0.0 && value < f64::from(limit);
        if !inside(x, self.width) || !inside(y, self.height) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "both lie in 0..limit of a u32, checked above"
        )]
        Some([x as u32, y as u32])
    }
}

/// A box of the fog, as [`FogOfWar::read`] copied it.
///
/// # Examples
///
/// ```
/// use enshwick::map::FogOfWar;
///
/// fn uncovered(fog: FogOfWar) -> usize {
///     let Ok(state) = fog.state() else { return 0 };
///     fog.read([0, 0], [state.width, state.height])
///         .map_or(0, |image| image.coverage().iter().filter(|&&c| c == 255).count())
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct FogImage {
    state: FogState,
    min: [u32; 2],
    size: [u32; 2],
    coverage: Vec<u8>,
}

impl FogImage {
    /// The fog as the copy found it.
    ///
    /// # Examples
    ///
    /// See [`FogImage`].
    #[must_use]
    pub const fn state(&self) -> FogState {
        self.state
    }

    /// The box's corner, in pixels.
    ///
    /// # Examples
    ///
    /// See [`FogImage`].
    #[must_use]
    pub const fn min(&self) -> [u32; 2] {
        self.min
    }

    /// The box's width and height, in pixels.
    ///
    /// # Examples
    ///
    /// See [`FogImage`].
    #[must_use]
    pub const fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The box's coverage, row by row.
    ///
    /// # Examples
    ///
    /// See [`FogImage`].
    #[must_use]
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }

    /// The coverage of fog pixel `pixel`; `None` outside the box.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::map::FogImage;
    ///
    /// fn first(image: &FogImage) -> Option<u8> {
    ///     image.at(image.min())
    /// }
    /// ```
    #[must_use]
    pub fn at(&self, pixel: [u32; 2]) -> Option<u8> {
        let x = pixel[0]
            .checked_sub(self.min[0])
            .filter(|&x| x < self.size[0])?;
        let y = pixel[1]
            .checked_sub(self.min[1])
            .filter(|&y| y < self.size[1])?;
        self.coverage
            .get(y as usize * self.size[0] as usize + x as usize)
            .copied()
    }
}

/// Why the fog does not read.
///
/// # Examples
///
/// ```
/// use enshwick::map::FogError;
///
/// assert_eq!(FogError::Missing.to_string(), "no fog of war to read");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FogError {
    /// The client is in no world, this game build lacks the fog, or the
    /// loader stopped copying it after a fault.
    #[error("no fog of war to read")]
    Missing,
    /// The box leaves the fog.
    #[error("the box leaves the fog of war")]
    Outside,
    /// The loader refused the read.
    #[error("the loader refused the fog read")]
    Refused,
}

impl FogError {
    fn check(status: Status) -> Result<(), Self> {
        match status {
            Status::OK => Ok(()),
            Status::MISSING => Err(Self::Missing),
            Status::OUTSIDE_WORLD => Err(Self::Outside),
            _ => Err(Self::Refused),
        }
    }
}

impl FogOfWar {
    pub(crate) fn open(load: &Load<'_>) -> Result<Self, Off> {
        let host = load.host();
        let status = (host.api().fog_open)(host.handle());
        crate::load::ok(status, Self { host }, || "the fog of war".to_owned())
    }

    /// The fog's size, scale and version.
    ///
    /// # Errors
    ///
    /// [`FogError::Missing`] while the client is in no world, on a game
    /// build without the fog, or after the loader stopped copying it.
    ///
    /// # Examples
    ///
    /// See [`FogImage`].
    pub fn state(self) -> Result<FogState, FogError> {
        let mut info = FogInfo::EMPTY;
        let status = (self.host.api().fog_read)(
            self.host.handle(),
            &raw mut info,
            0,
            0,
            0,
            0,
            core::ptr::null_mut(),
            0,
        );
        FogError::check(status)?;
        Ok(FogState::from_abi(&info))
    }

    /// Copies the box of `size` pixels at `min`.
    ///
    /// # Errors
    ///
    /// [`FogError::Outside`] for a box past the fog; otherwise as
    /// [`FogOfWar::state`].
    ///
    /// # Examples
    ///
    /// See [`FogImage`].
    pub fn read(self, min: [u32; 2], size: [u32; 2]) -> Result<FogImage, FogError> {
        let len = (size[0] as usize)
            .checked_mul(size[1] as usize)
            .ok_or(FogError::Outside)?;
        let capacity = u32::try_from(len).map_err(|_| FogError::Outside)?;
        let mut coverage = vec![0u8; len];
        let mut info = FogInfo {
            header: Header::of::<FogInfo>(),
            ..FogInfo::EMPTY
        };
        let status = (self.host.api().fog_read)(
            self.host.handle(),
            &raw mut info,
            min[0],
            min[1],
            size[0],
            size[1],
            coverage.as_mut_ptr(),
            capacity,
        );
        FogError::check(status)?;
        Ok(FogImage {
            state: FogState::from_abi(&info),
            min,
            size,
            coverage,
        })
    }

    /// The coverage where `position` is, in metres; `None` outside the fog.
    ///
    /// # Errors
    ///
    /// As [`FogOfWar::state`].
    ///
    /// # Examples
    ///
    /// See [`FogOfWar`].
    pub fn coverage(self, position: [f64; 3]) -> Result<Option<u8>, FogError> {
        let Some(pixel) = self.state()?.pixel_of(position) else {
            return Ok(None);
        };
        match self.read(pixel, [1, 1]) {
            Ok(image) => Ok(image.at(pixel)),
            Err(FogError::Outside) => Ok(None),
            Err(why) => Err(why),
        }
    }
}
