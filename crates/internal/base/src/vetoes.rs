//! What a mod's filter answers for a request the game is about to carry out.

use std::borrow::Cow;

use crate::script::{Answer, Error, Ty, Value};

/// What a filter answers for a request the game is about to carry out, from
/// [`Load::on_craft`](../struct.Load.html#method.on_craft),
/// [`Load::on_build`](../struct.Load.html#method.on_build) and the other
/// filters; each takes a `bool`, an `Option<bool>` or a `Verdict`.
///
/// From Luau a filter answers `false` to refuse, a string to refuse with
/// that reason, `true` to allow and nil to pass; any other value refuses,
/// and a script error passes. The name is enshwick's.
///
/// # Examples
///
/// ```
/// use enshwick::vetoes::Verdict;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.on_build(|placement| {
///         if placement.position[1] >= 0.0 {
///             Verdict::Pass
///         } else {
///             Verdict::refuse_because("build.below_sea")
///         }
///     })
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// The filter leaves the request to the game and the other filters.
    Pass,
    /// The filter allows the request: where a filter can overrule the game,
    /// as [`Load::on_skill`](../struct.Load.html#method.on_skill) unlocks a
    /// skill, the game's own refusal goes; elsewhere it is [`Verdict::Pass`].
    Allow,
    /// The request is refused, as the game refuses it.
    Refuse,
    /// The request is refused, and the player reads why where the game
    /// shows a reason, as under the build cursor: a key of the mod's texts,
    /// shown in the player's language, or plain words, at most 120 bytes.
    /// Elsewhere it is [`Verdict::Refuse`].
    RefuseBecause(Cow<'static, str>),
}

impl Verdict {
    /// A refusal the player reads `reason` for; a `&'static str` costs no
    /// allocation, as the build cursor's check needs.
    ///
    /// # Examples
    ///
    /// See [`Verdict`].
    #[must_use]
    pub fn refuse_because(reason: impl Into<Cow<'static, str>>) -> Self {
        Self::RefuseBecause(reason.into())
    }

    /// Whether the request goes on: every verdict but a refusal.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::vetoes::Verdict;
    ///
    /// assert!(Verdict::Pass.passes() && !Verdict::refuse_because("no").passes());
    /// ```
    #[must_use]
    pub const fn passes(&self) -> bool {
        !matches!(self, Self::Refuse | Self::RefuseBecause(_))
    }

    /// The verdict as an overruling answer: `None` to leave it to the game,
    /// else whether the request goes on.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::vetoes::Verdict;
    ///
    /// assert_eq!((Verdict::Pass.decision(), Verdict::Allow.decision()), (None, Some(true)));
    /// ```
    #[must_use]
    pub const fn decision(&self) -> Option<bool> {
        match self {
            Self::Pass => None,
            Self::Allow => Some(true),
            Self::Refuse | Self::RefuseBecause(_) => Some(false),
        }
    }
}

impl From<bool> for Verdict {
    fn from(allowed: bool) -> Self {
        if allowed { Self::Allow } else { Self::Refuse }
    }
}

impl From<Option<bool>> for Verdict {
    fn from(decision: Option<bool>) -> Self {
        decision.map_or(Self::Pass, Self::from)
    }
}

impl Answer for Verdict {
    const TY: &'static Ty = &Ty::Data;

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::Nil) | Err(_) => Self::Pass,
            Ok(Value::Bool(true)) => Self::Allow,
            Ok(Value::String(reason)) => Self::RefuseBecause(reason.into()),
            // A mistyped answer refuses, so the author sees it at once.
            Ok(_) => Self::Refuse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_s_answer_reads_as_the_verdict_it_names() {
        let answer = Verdict::from_answer;
        assert_eq!(answer(Ok(Value::Nil)), Verdict::Pass);
        assert_eq!(answer(Ok(Value::Bool(true))), Verdict::Allow);
        assert_eq!(answer(Ok(Value::Bool(false))), Verdict::Refuse);
        assert_eq!(
            answer(Ok(Value::String("build.below_sea".into()))),
            Verdict::refuse_because("build.below_sea")
        );
        assert_eq!(answer(Ok(Value::Number(1.0))), Verdict::Refuse);
        assert_eq!(answer(Err(Error::new("boom"))), Verdict::Pass);
    }

    #[test]
    fn a_bool_and_an_option_convert_to_the_verdict_they_mean() {
        assert_eq!(Verdict::from(false), Verdict::Refuse);
        assert_eq!(Verdict::from(None), Verdict::Pass);
        assert_eq!(Verdict::from(Some(true)).decision(), Some(true));
    }
}
