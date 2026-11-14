//! The bounds and rates every input from a peer passes at the boundary: one
//! constant per bound, with its reason, and one token bucket for every rate,
//! so the loader and the fake host refuse the same input.

use std::time::Instant;

use rustc_hash::{FxBuildHasher, FxHashMap};

/// The bytes a player's name takes at most: the game's account names fit,
/// and a remote caller's label is cut to the same room.
pub const NAME_BYTES: usize = 64;

/// The bytes of arguments one action run a client sends takes at most: room
/// for several texts of the most bytes a text argument takes, so a changed
/// client cannot make the server decode more.
pub const MAX_ARGS: usize = 2_048;

/// The most players one show or close of a mod's window hands the loader
/// in a call; `Actions::show` splits a longer list.
pub const MAX_CALL_PLAYERS: usize = 64;

/// The most shows, updates and closes of mods' windows the loader keeps for
/// its next step, one for each player; a call past it is refused.
pub const MAX_WAITING_WINDOWS: usize = 256;

/// The accounts a [`Rated`] keeps before it forgets the ones whose bucket is
/// full again; the game holds far fewer players.
pub const RATED_ACCOUNTS: usize = 1_024;

/// `name` cut to [`NAME_BYTES`] on a character boundary.
///
/// # Examples
///
/// ```
/// use enshwick_base::peer::{NAME_BYTES, cut_name};
///
/// assert_eq!(cut_name("Ann"), "Ann");
/// assert_eq!(cut_name(&"й".repeat(40)).len(), NAME_BYTES);
/// ```
#[must_use]
pub fn cut_name(name: &str) -> &str {
    &name[..name.floor_char_boundary(NAME_BYTES)]
}

/// Whether `text` holds no control character but the tab that separates the
/// fields of an action's arguments.
///
/// # Examples
///
/// ```
/// use enshwick_base::peer::without_controls;
///
/// assert!(without_controls("tBan\tb1"));
/// assert!(!without_controls("tline\nbreak"));
/// ```
#[must_use]
pub fn without_controls(text: &str) -> bool {
    !text.chars().any(|c| c.is_control() && c != '\t')
}

/// Whether a client's action arguments `args` pass the peer bounds: within
/// [`MAX_ARGS`] and [without a control character](without_controls); the
/// mod reads each value against its argument once after.
///
/// # Examples
///
/// ```
/// use enshwick_base::peer::{MAX_ARGS, args_in_bounds};
///
/// assert!(args_in_bounds("tBan for griefing\tb1"));
/// assert!(!args_in_bounds(&"t".repeat(MAX_ARGS + 1)));
/// ```
#[must_use]
pub fn args_in_bounds(args: &str) -> bool {
    args.len() <= MAX_ARGS && without_controls(args)
}

/// A rate: so many a second on average, after a burst of so many at once.
///
/// # Examples
///
/// ```
/// use enshwick_base::peer::Rate;
///
/// const RUNS: Rate = Rate::new(5.0, 10.0);
/// assert_eq!(RUNS.burst, 10.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    /// What one second gives back.
    pub per_second: f64,
    /// The most taken at once, from a full bucket.
    pub burst: f64,
}

impl Rate {
    /// A rate of `per_second` after a burst of `burst`.
    ///
    /// # Examples
    ///
    /// See [`Rate`].
    #[must_use]
    pub const fn new(per_second: f64, burst: f64) -> Self {
        Self { per_second, burst }
    }
}

/// The message rate of one peer of the loader's wire, any message.
pub const MESSAGE_RATE: Rate = Rate::new(20.0, 40.0);

/// The queries one player who is no administrator sends: each answer can be
/// a whole control message, so the frame rate alone would let a player make
/// the server send far more than the player sends.
pub const QUERY_RATE: Rate = Rate::new(2.0, 8.0);

/// The action runs one player who is no administrator sends, from the
/// interface, a window or chat: each costs the server a handler call and a
/// notice, and a control sends one a key press.
pub const RUN_RATE: Rate = Rate::new(5.0, 10.0);

/// The settings changes one administrator sends: each writes a file.
pub const EDIT_RATE: Rate = Rate::new(4.0, 1.0);

/// The chat lines of one account raised to mods: the filters still run on
/// every line, so only a flood loses one.
pub const CHAT_LINE_RATE: Rate = Rate::new(8.0, 8.0);

/// One token bucket: what a peer may still send now, refilled at a
/// [`Rate`] up to its burst.
///
/// # Examples
///
/// ```
/// use std::time::Instant;
/// use enshwick_base::peer::{Bucket, Rate};
///
/// let rate = Rate::new(1.0, 2.0);
/// let now = Instant::now();
/// let mut bucket = Bucket::full(rate, now);
/// assert!(bucket.admit_at(now, rate));
/// assert!(bucket.admit_at(now, rate));
/// assert!(!bucket.admit_at(now, rate));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Bucket {
    tokens: f64,
    seen: Instant,
}

impl Bucket {
    /// A bucket holding a whole burst of `rate` at `now`.
    ///
    /// # Examples
    ///
    /// See [`Bucket`].
    #[must_use]
    pub const fn full(rate: Rate, now: Instant) -> Self {
        Self::with_tokens(rate.burst, now)
    }

    /// A bucket holding `tokens` at `now`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use enshwick_base::peer::{Bucket, RUN_RATE};
    ///
    /// let now = Instant::now();
    /// assert!(!Bucket::with_tokens(0.0, now).admit_at(now, RUN_RATE));
    /// ```
    #[must_use]
    pub const fn with_tokens(tokens: f64, now: Instant) -> Self {
        Self { tokens, seen: now }
    }

    /// What the bucket holds, as last counted.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use enshwick_base::peer::{Bucket, Rate};
    ///
    /// assert_eq!(Bucket::full(Rate::new(1.0, 3.0), Instant::now()).tokens(), 3.0);
    /// ```
    #[must_use]
    pub const fn tokens(&self) -> f64 {
        self.tokens
    }

    /// Takes one at `now`, after what `rate` gave back since the last time;
    /// false when the bucket is spent.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use enshwick_base::peer::{Bucket, Rate};
    ///
    /// let now = Instant::now();
    /// let mut bucket = Bucket::with_tokens(0.0, now);
    /// assert!(bucket.admit_at(now + Duration::from_secs(1), Rate::new(1.0, 1.0)));
    /// ```
    pub fn admit_at(&mut self, now: Instant, rate: Rate) -> bool {
        let gained = now.saturating_duration_since(self.seen).as_secs_f64() * rate.per_second;
        self.seen = now;
        self.tokens = (self.tokens + gained).min(rate.burst);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }

    /// Gives back one taken for something that did not go, up to the burst
    /// of `rate`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use enshwick_base::peer::{Bucket, Rate};
    ///
    /// let (rate, now) = (Rate::new(1.0, 1.0), Instant::now());
    /// let mut bucket = Bucket::full(rate, now);
    /// assert!(bucket.admit_at(now, rate));
    /// bucket.refund(rate);
    /// assert!(bucket.admit_at(now, rate));
    /// ```
    pub fn refund(&mut self, rate: Rate) {
        self.tokens = (self.tokens + 1.0).min(rate.burst);
    }
}

/// What a [`Rated`] made of one input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admitted {
    /// Within the rate: serve it.
    Yes,
    /// Over the rate, the first since the account's last admitted input:
    /// answer it once.
    First,
    /// Over the rate again: drop it unanswered, so a flood never makes the
    /// server send as much back.
    Again,
}

/// One bucket per account at one [`Rate`], bounded to [`RATED_ACCOUNTS`]:
/// the one rate limiter of every input path a player reaches.
///
/// # Examples
///
/// ```
/// use std::time::Instant;
/// use enshwick_base::peer::{Admitted, Rate, Rated};
///
/// let mut rated = Rated::new(Rate::new(1.0, 1.0));
/// let now = Instant::now();
/// assert_eq!(rated.admit(7, now), Admitted::Yes);
/// assert_eq!(rated.admit(7, now), Admitted::First);
/// assert_eq!(rated.admit(7, now), Admitted::Again);
/// assert_eq!(rated.admit(8, now), Admitted::Yes, "each account has its own");
/// ```
#[derive(Debug)]
pub struct Rated {
    rate: Rate,
    /// Each account's bucket and whether its last input was refused.
    buckets: FxHashMap<u64, (Bucket, bool)>,
}

impl Rated {
    /// No account seen yet, at `rate`.
    ///
    /// # Examples
    ///
    /// See [`Rated`].
    #[must_use]
    pub const fn new(rate: Rate) -> Self {
        Self {
            rate,
            buckets: FxHashMap::with_hasher(FxBuildHasher),
        }
    }

    /// Takes one input of account `key` at `now`.
    ///
    /// # Examples
    ///
    /// See [`Rated`].
    pub fn admit(&mut self, key: u64, now: Instant) -> Admitted {
        if self.buckets.len() >= RATED_ACCOUNTS && !self.buckets.contains_key(&key) {
            self.forget_full(now);
            if self.buckets.len() >= RATED_ACCOUNTS {
                return Admitted::Again;
            }
        }
        let rate = self.rate;
        let (bucket, refused) = self
            .buckets
            .entry(key)
            .or_insert_with(|| (Bucket::full(rate, now), false));
        if bucket.admit_at(now, rate) {
            *refused = false;
            Admitted::Yes
        } else if core::mem::replace(refused, true) {
            Admitted::Again
        } else {
            Admitted::First
        }
    }

    /// Forgets account `key`, which left.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use enshwick_base::peer::{Admitted, Rate, Rated};
    ///
    /// let mut rated = Rated::new(Rate::new(1.0, 1.0));
    /// let now = Instant::now();
    /// rated.admit(7, now);
    /// rated.forget(7);
    /// assert_eq!(rated.admit(7, now), Admitted::Yes);
    /// ```
    pub fn forget(&mut self, key: u64) {
        self.buckets.remove(&key);
    }

    /// Forgets every account, as when the world goes away.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use enshwick_base::peer::{Admitted, Rate, Rated};
    ///
    /// let mut rated = Rated::new(Rate::new(1.0, 1.0));
    /// let now = Instant::now();
    /// rated.admit(7, now);
    /// rated.clear();
    /// assert_eq!(rated.admit(7, now), Admitted::Yes);
    /// ```
    pub fn clear(&mut self) {
        self.buckets.clear();
    }

    /// Forgets the accounts whose bucket `now` would hold a whole burst
    /// again, which lose nothing by it.
    #[cold]
    fn forget_full(&mut self, now: Instant) {
        let rate = self.rate;
        self.buckets.retain(|_, (bucket, _)| {
            let mut probe = *bucket;
            let _taken = probe.admit_at(now, rate);
            probe.tokens + 1.0 < rate.burst
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_bucket_lets_a_burst_through_then_refills_at_its_rate() {
        let rate = Rate::new(5.0, 10.0);
        let start = Instant::now();
        let mut bucket = Bucket::full(rate, start);
        let burst = (0..100).filter(|_| bucket.admit_at(start, rate)).count();
        assert_eq!(burst, 10);
        let second = start + Duration::from_secs(1);
        let refilled = (0..100).filter(|_| bucket.admit_at(second, rate)).count();
        assert_eq!(refilled, 5);
    }

    #[test]
    fn a_bucket_never_holds_more_than_its_burst_after_a_long_wait() {
        let rate = Rate::new(5.0, 2.0);
        let start = Instant::now();
        let mut bucket = Bucket::with_tokens(0.0, start);
        let later = start + Duration::from_secs(3_600);
        let taken = (0..10).filter(|_| bucket.admit_at(later, rate)).count();
        assert_eq!(taken, 2);
    }

    #[test]
    fn a_flood_is_answered_once_until_an_input_is_admitted_again() {
        let mut rated = Rated::new(Rate::new(1.0, 1.0));
        let start = Instant::now();
        assert_eq!(rated.admit(1, start), Admitted::Yes);
        assert_eq!(rated.admit(1, start), Admitted::First);
        assert_eq!(rated.admit(1, start), Admitted::Again);
        let later = start + Duration::from_secs(1);
        assert_eq!(rated.admit(1, later), Admitted::Yes);
        assert_eq!(rated.admit(1, later), Admitted::First, "a new flood");
    }

    #[test]
    fn the_rate_forgets_rested_accounts_once_it_holds_its_bound() {
        let mut rated = Rated::new(Rate::new(1.0, 2.0));
        let start = Instant::now();
        for key in 0..u64::try_from(RATED_ACCOUNTS).unwrap_or(u64::MAX) {
            assert_eq!(rated.admit(key, start), Admitted::Yes);
        }
        let rested = start + Duration::from_secs(10);
        assert_eq!(rated.admit(u64::MAX, rested), Admitted::Yes);
        assert_eq!(rated.buckets.len(), 1, "every rested account was dropped");
    }

    #[test]
    fn busy_accounts_are_kept_and_a_new_one_waits_while_the_rate_is_full() {
        let mut rated = Rated::new(Rate::new(1.0, 2.0));
        let start = Instant::now();
        for key in 0..u64::try_from(RATED_ACCOUNTS).unwrap_or(u64::MAX) {
            rated.admit(key, start);
        }
        assert_eq!(rated.admit(u64::MAX, start), Admitted::Again);
        assert_eq!(rated.buckets.len(), RATED_ACCOUNTS, "the bound holds");
        rated.admit(0, start);
        assert_eq!(
            rated.admit(0, start),
            Admitted::First,
            "a spent account was not forgotten, so a flood cannot reset it"
        );
    }
}
