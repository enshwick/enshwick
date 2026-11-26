//! What the game pays a player for a quest, which a mod's filter keeps,
//! changes or refuses before the game pays it.

use core::ffi::c_void;

use crate::abi::QuestPayment as RawPayment;
use crate::content::knowledge::ExperienceValue;
use crate::knowledge::KnowledgeId;
use crate::script::{Answer, Error, FromScript as _, Ty, Value};
use crate::{Entity, Off};

/// What the game pays a player for a quest, as the filters before left it.
///
/// The game pays the quest's experience values and sets the quest's bool;
/// the items its journal draws as rewards it never pays.
///
/// # Examples
///
/// See `Load::on_quest_reward`.
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct QuestPayment {
    /// The player's character.
    pub player: Entity,
    /// The quest.
    pub quest: KnowledgeId,
    /// The experience values it pays, in order; one twice pays it twice.
    pub experience: Vec<ExperienceValue>,
    /// The most experience values the game pays on this build; the loader
    /// drops the rest with a warning.
    pub limit: u32,
    /// Whether the game pays it so far: `false` once a filter before
    /// refused it.
    pub allowed: bool,
}

/// What a quest reward filter does with a payment.
///
/// From Luau a filter answers `false` to refuse, `true` to pay what a
/// filter before refused, a list of experience values to pay those
/// instead, and nil to keep it.
///
/// # Examples
///
/// See `Load::on_quest_reward`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QuestPay {
    /// The payment goes on as it is, refused or not.
    Keep,
    /// The game pays nothing, and the quest still completes.
    Refuse,
    /// The game pays the payment as the filters before left its values,
    /// though one of them refused it.
    Allow,
    /// The game pays these experience values instead, which the filters
    /// after this one see; a payment a filter before refused stays refused.
    Pay(Vec<ExperienceValue>),
}

impl Answer for QuestPay {
    const TY: &'static Ty = &Ty::Data;

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::Bool(false)) => Self::Refuse,
            Ok(Value::Bool(true)) => Self::Allow,
            Ok(list @ Value::List(_)) => {
                Vec::<ExperienceValue>::from_value(list).map_or(Self::Keep, Self::Pay)
            }
            _ => Self::Keep,
        }
    }
}

impl QuestPayment {
    fn of(raw: &RawPayment) -> Self {
        let count = usize::try_from(raw.count).map_or(0, |count| count.min(raw.experience.len()));
        Self {
            player: raw.entity,
            quest: raw.quest,
            experience: raw.experience[..count]
                .iter()
                .map(|&id| ExperienceValue::new(id))
                .collect(),
            limit: raw.limit,
            allowed: raw.allowed != 0,
        }
    }
}

/// Writes `pay` into the loader's payment `raw`, and the answer the
/// loader takes for it. A list longer than the payment holds is cut there.
fn answer(pay: QuestPay, raw: &mut RawPayment) -> u8 {
    match pay {
        QuestPay::Keep => crate::abi::FILTER_PASS,
        QuestPay::Refuse => crate::abi::FILTER_REFUSE,
        QuestPay::Allow => crate::abi::FILTER_YES,
        QuestPay::Pay(experience) => {
            let kept = experience.len().min(raw.experience.len());
            for (slot, value) in raw.experience.iter_mut().zip(&experience) {
                *slot = value.get();
            }
            raw.count = u32::try_from(kept).unwrap_or(0);
            crate::abi::FILTER_PASS
        }
    }
}

unsafe extern "system" fn run_payment<F>(call: *mut RawPayment, user: *mut c_void) -> u8
where
    F: Fn(&QuestPayment) -> QuestPay,
{
    // SAFETY: the loader passes a valid, writable payment for the duration
    // of this call, which no other thread touches meanwhile.
    let Some(raw) = (unsafe { call.as_mut() }) else {
        return crate::abi::FILTER_PASS;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let filter = unsafe { &*user.cast::<F>() };
    let payment = QuestPayment::of(raw);
    crate::host::guard(
        "panic in a quest reward filter",
        || crate::abi::FILTER_PASS,
        || answer(filter(&payment), raw),
    )
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_quest_reward`.
    #[doc(hidden)]
    fn on_quest_reward<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&QuestPayment) -> QuestPay + Send + Sync + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn on_quest_reward<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&QuestPayment) -> QuestPay + Send + Sync + 'static,
    {
        let host = self.host();
        crate::load::hand_over(
            filter,
            |user| (host.api().quest_reward_filter)(host.handle(), Some(run_payment::<F>), user),
            "quest reward filter",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(count: u32) -> RawPayment {
        enshwick_abi::headed!(RawPayment {
            entity: Entity::from_engine(2),
            quest: KnowledgeId::new(0x51),
            count,
            limit: 3,
            allowed: 1,
            reserved: [0; 3],
            experience: [7; RawPayment::MAX],
        })
    }

    #[test]
    fn a_payment_reads_only_the_values_in_use() {
        assert_eq!(
            QuestPayment::of(&raw(2)).experience,
            [ExperienceValue::new(7); 2]
        );
        assert!(
            QuestPayment::of(&raw(u32::MAX)).experience.len() == RawPayment::MAX,
            "cut at what it holds"
        );
    }

    #[test]
    fn a_verdict_keeps_refuses_allows_or_writes_the_values_it_pays() {
        let mut call = raw(1);
        assert_eq!(answer(QuestPay::Keep, &mut call), crate::abi::FILTER_PASS);
        assert_eq!(
            answer(QuestPay::Refuse, &mut call),
            crate::abi::FILTER_REFUSE
        );
        assert_eq!(answer(QuestPay::Allow, &mut call), crate::abi::FILTER_YES);
        let twice = vec![ExperienceValue::new(9); 2];
        assert_eq!(
            answer(QuestPay::Pay(twice), &mut call),
            crate::abi::FILTER_PASS,
            "new values leave a refusal before standing"
        );
        assert_eq!((call.count, &call.experience[..2]), (2, &[9, 9][..]));
        let long = vec![ExperienceValue::new(1); RawPayment::MAX + 4];
        let _yes = answer(QuestPay::Pay(long), &mut call);
        assert_eq!(call.count as usize, RawPayment::MAX);
    }

    #[test]
    fn a_script_answers_false_to_refuse_true_to_allow_a_list_to_pay_and_nil_to_keep() {
        assert_eq!(
            QuestPay::from_answer(Ok(Value::Bool(false))),
            QuestPay::Refuse
        );
        assert_eq!(
            QuestPay::from_answer(Ok(Value::Bool(true))),
            QuestPay::Allow
        );
        assert!(QuestPayment::of(&raw(1)).allowed);
        assert_eq!(QuestPay::from_answer(Ok(Value::Nil)), QuestPay::Keep);
        assert_eq!(
            QuestPay::from_answer(Ok(Value::List(vec![Value::Number(5.0)]))),
            QuestPay::Pay(vec![ExperienceValue::new(5)])
        );
    }
}
