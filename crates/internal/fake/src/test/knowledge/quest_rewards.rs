//! What the fake pays for a quest: the mod's quest reward filters, run as
//! the loader runs them before the game pays.

use enshwick_core::abi::{FILTER_REFUSE, FILTER_YES, QuestPayment};

use crate::test::{Installed, table};

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Runs the mod's quest reward filters on `payment`, as the loader runs
    /// them before the game pays a quest, and returns what the game would
    /// pay: each filter sees the payment as the filters before left it,
    /// and its refusal stands unless a later filter answers yes. A count
    /// past the `limit` the payment came with or [`QuestPayment::MAX`] is
    /// cut there, and a refused payment pays none, as the loader pays.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, QuestPayment};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let quest = QuestPayment {
    ///     header: Header::of::<QuestPayment>(),
    ///     entity: enshwick::Entity(2),
    ///     quest: enshwick::knowledge::KnowledgeId::new(0x51),
    ///     count: 1,
    ///     limit: 3,
    ///     allowed: 1,
    ///     reserved: [0; 3],
    ///     experience: [0x77; QuestPayment::MAX],
    /// };
    /// assert_eq!(loader.pay_quest(quest), quest, "no filters, the game's payment");
    /// ```
    #[must_use]
    pub fn pay_quest(&self, mut payment: QuestPayment) -> QuestPayment {
        let filters = table::with(Vec::new(), |s| s.knowledge.quest_reward_filters.clone());
        let most = payment
            .limit
            .min(u32::try_from(QuestPayment::MAX).unwrap_or(u32::MAX));
        for filter in filters {
            // SAFETY: the filter the mod registered, with its pointer and the
            // payment, which lives for the call.
            match filter.run(|callback, user| unsafe { callback(&raw mut payment, user) }) {
                FILTER_YES => payment.allowed = 1,
                FILTER_REFUSE => payment.allowed = 0,
                _ => {}
            }
        }
        payment.count = if payment.allowed == 0 {
            0
        } else {
            payment.count.min(most)
        };
        payment
    }
}
