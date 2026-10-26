//! Game rates: rates a build lacks, and the factor in force for each.

use super::{FakeHost, Installed, table};
use crate::rates::{Rate, combine};

impl FakeHost {
    /// A build that lacks what `rate` needs: declaring it answers `Off`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::rates::Rate;
    ///
    /// let loader = enshwick::test::FakeHost::new().without_rate(Rate::WorkshopSpeed).install();
    /// assert_eq!(loader.rate(Rate::WorkshopSpeed), None);
    /// ```
    #[must_use]
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
    /// assert_eq!(loader.rate(Rate::ResourceDrops), None);
    /// ```
    #[must_use]
    pub fn rate(&self, rate: Rate) -> Option<f32> {
        let id = rate.id();
        table::with(None, |s| {
            s.rates
                .iter()
                .find(|&&(declared, _)| declared == id)
                .map(|&(_, factor)| combine([factor]))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::abi::{HostApi, ModHandle, Status};
    use crate::rates::{ExperienceSource, Rate};
    use crate::test::FakeHost;
    use crate::test::table::TABLE;

    fn api(_loader: &crate::test::Installed) -> &'static HostApi {
        &TABLE
    }

    #[test]
    fn a_declared_rate_starts_vanilla_and_takes_the_factor_set() {
        let loader = FakeHost::new().install();
        let api = api(&loader);
        let rate = Rate::Experience(ExperienceSource::Mining);
        assert_eq!((api.rate)(ModHandle(1), rate.id()), Status::OK);
        assert_eq!(loader.rate(rate), Some(1.0));
        assert_eq!((api.set_rate)(ModHandle(1), rate.id(), 2.5), Status::OK);
        assert_eq!(loader.rate(rate), Some(2.5));
    }

    #[test]
    fn the_fake_refuses_what_the_loader_refuses() {
        let loader = FakeHost::new().without_rate(Rate::WorkshopSpeed).install();
        let api = api(&loader);
        let speed = Rate::WorkshopSpeed.id();
        let drops = Rate::ResourceDrops.id();
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
        assert_eq!(loader.rate(Rate::ResourceDrops), Some(1.0));
    }
}
