//! Game rates: rates a build lacks, and the factor in force for each.

use crate::abi::RateId;
#[cfg(feature = "unstable")]
use crate::rates::Rate;
use crate::rates::combine;

use super::{FakeHost, Installed, table};

impl FakeHost {
    /// A build that lacks what `rate` needs: declaring it answers `Off`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::rates::Rate;
    ///
    /// let loader = enshwick::test::FakeHost::new().without_rate(Rate::FactoryProductionSpeed).install();
    /// assert_eq!(loader.rate(Rate::FactoryProductionSpeed), None);
    /// ```
    #[must_use]
    #[cfg(feature = "unstable")]
    pub fn without_rate(mut self, rate: Rate) -> Self {
        self.missing_rates.insert(rate.id());
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The factor the loader would put in force for `rate`, combined as the
    /// loader combines every mod's factors; `None` when the mod did not
    /// declare it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::rates::Rate;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.rate(Rate::ResourceDropStackAmount), None);
    /// ```
    #[must_use]
    #[cfg(feature = "unstable")]
    pub fn rate(&self, rate: Rate) -> Option<f32> {
        self.rate_of(rate.id())
    }

    /// The factor in force for the rate `id`, combined as the loader combines it.
    pub(crate) fn rate_of(&self, id: RateId) -> Option<f32> {
        table::with(None, |s| {
            s.rates
                .iter()
                .find(|&&(declared, _)| declared == id)
                .map(|&(_, factor)| combine([factor]))
        })
    }
}

#[cfg(test)]
#[cfg(feature = "unstable")]
mod tests {
    use crate::abi::{HostApi, ModHandle, Status};
    use crate::rates::{ExperienceGroup, Rate};
    use crate::test::FakeHost;
    use crate::test::table::TABLE;

    fn api(_loader: &crate::test::Installed) -> &'static HostApi {
        &TABLE
    }

    #[test]
    fn a_declared_rate_starts_vanilla_and_takes_the_factor_set() {
        let loader = FakeHost::new().install();
        let api = api(&loader);
        let rate = Rate::Experience(ExperienceGroup::Mining);
        assert_eq!((api.rate)(ModHandle(1), rate.id()), Status::OK);
        assert_eq!(loader.rate(rate), Some(1.0));
        assert_eq!((api.set_rate)(ModHandle(1), rate.id(), 2.5), Status::OK);
        assert_eq!(loader.rate(rate), Some(2.5));
    }

    #[test]
    fn the_fake_refuses_what_the_loader_refuses() {
        let loader = FakeHost::new()
            .without_rate(Rate::FactoryProductionSpeed)
            .install();
        let api = api(&loader);
        let speed = Rate::FactoryProductionSpeed.id();
        let drops = Rate::ResourceDropStackAmount.id();
        assert_eq!((api.rate)(ModHandle(1), speed), Status::MISSING);
        assert_eq!(
            (api.rate)(ModHandle(1), crate::abi::RateId(0)),
            Status::BAD_ARGS
        );
        assert_eq!((api.set_rate)(ModHandle(1), drops, 2.0), Status::BAD_ARGS);
        assert_eq!((api.rate)(ModHandle(1), drops), Status::OK);
        for factor in [-0.5, 100.5, f32::NAN, f32::INFINITY] {
            assert_eq!(
                (api.set_rate)(ModHandle(1), drops, factor),
                Status::BAD_ARGS
            );
        }
        assert_eq!(loader.rate(Rate::ResourceDropStackAmount), Some(1.0));
    }
}
