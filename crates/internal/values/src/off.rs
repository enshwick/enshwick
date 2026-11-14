//! Why a mod turns itself off.

use std::borrow::Cow;

use crate::reason;

/// Why a mod turns itself off during initialisation; the loader logs the
/// reason and keeps the game vanilla where the mod would have acted.
///
/// Every error type of this crate turns into an `Off` with `?`, its message
/// the reason.
///
/// # Examples
///
/// ```
/// let off = enshwick::Off::new("this game build is not supported");
/// assert_eq!(off.to_string(), "this game build is not supported");
///
/// fn calls(load: &enshwick::Load<'_>) -> Result<usize, enshwick::Off> {
///     let pattern = enshwick::code::Pattern::parse("E8 ?? ?? ?? ??")?;
///     Ok(pattern.find_all(load.code()?).count())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct Off {
    reason: Cow<'static, str>,
    /// The reason as the loader reads a text of the mod's texts, when it is
    /// one.
    text: Option<String>,
}

impl Off {
    /// An `Off` with a reason for the log.
    ///
    /// # Examples
    ///
    /// See [`Off`].
    #[must_use]
    pub fn new(reason: impl Into<Cow<'static, str>>) -> Self {
        Self {
            reason: reason.into(),
            text: None,
        }
    }

    /// An `Off` whose reason is the text of `key` in the mod's texts with
    /// each `{name}` filled from `args`, so the Mods screen, the status
    /// summary and the log show it in the reader's language.
    ///
    /// The mod hands its texts over with [`Load::texts`](struct.Load.html#method.texts) first; a reason
    /// whose key has no text shows as the key and its values.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let _texts = load.texts_from("[en]\n\"glow.off\" = \"No glow on this game version.\"\n")?;
    ///     let glow = load.feature("glow", |load| {
    ///         load.symbol("lantern.glow").map_err(|_| enshwick::Off::text("glow.off", &[]))
    ///     });
    ///     let _ = glow;
    ///     Ok(())
    /// }
    ///
    /// let off = enshwick::Off::text("stalls.min", &[("min", "2")]);
    /// assert_eq!(off.to_string(), "stalls.min (min = 2)");
    /// ```
    #[must_use]
    pub fn text(key: &str, args: &[(&str, &str)]) -> Self {
        Self {
            reason: reason::show(key, args).into(),
            text: Some(reason::encode(key, args)),
        }
    }

    /// An `Off` that shows as `reason` and that the loader reads as `text`,
    /// a text of the mod's texts in the form [`reason::encode`] writes.
    #[doc(hidden)]
    #[must_use]
    pub fn with_text(reason: impl Into<Cow<'static, str>>, text: Option<String>) -> Self {
        Self {
            reason: reason.into(),
            text,
        }
    }

    /// The reason as the loader reads it: a text as its key and values,
    /// any other as its words.
    #[doc(hidden)]
    #[must_use]
    pub fn to_reason(&self) -> &str {
        self.text.as_deref().unwrap_or(&self.reason)
    }
}
