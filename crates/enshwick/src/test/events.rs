//! Events: the mod's subscriptions, raised loader and game events, mod events.

use super::Installed;
use super::table;
use crate::abi::Event;
use crate::abi::Header;
use crate::events::Kind;

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The events the mod subscribed to, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.subscriptions().is_empty());
    /// ```
    #[must_use]
    pub fn subscriptions(&self) -> Vec<Event> {
        table::with(Vec::new(), |s| {
            s.subscriptions.iter().map(|s| s.0).collect()
        })
    }

    /// Raises the event `E` with `data` to every handler of the mod, on this
    /// thread; returns how many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, TickData};
    /// use enshwick::events;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let tick = TickData { header: Header::of::<TickData>(), step: 1 };
    /// assert_eq!(loader.raise::<events::Ticked>(&tick), 0);
    /// assert_eq!(loader.raise::<events::ShutdownStarted>(&()), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub fn raise<E: Kind>(&self, data: &E::Data) -> usize {
        // SAFETY: `data` is the event's data and lives for the call.
        unsafe { self.raise_raw(E::EVENT, core::ptr::from_ref(data).cast()) }
    }

    /// Raises `event` with raw `data`, for events this crate has no type for;
    /// returns how many handlers ran.
    ///
    /// # Safety
    ///
    /// - `data` is null when the loader passes no data with `event`, and
    ///   otherwise points at the data struct the loader passes with it.
    /// - That struct is readable, aligned and unchanged for the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Event;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// // SAFETY: the event carries no data.
    /// let ran = unsafe { loader.raise_raw(Event::WORLD_SAVED, core::ptr::null()) };
    /// assert_eq!(ran, 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub unsafe fn raise_raw(&self, event: Event, data: *const core::ffi::c_void) -> usize {
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.subscriptions
                .iter()
                .filter(|s| s.0 == event)
                .map(|s| (s.1, s.2))
                .collect()
        });
        for &(callback, user) in &handlers {
            // SAFETY: the caller guarantees `data` is null or the event's data
            // struct, readable for the call; `user` is the pointer the mod
            // subscribed `callback` with.
            unsafe { callback(event, data, core::ptr::with_exposed_provenance_mut(user)) };
        }
        handlers.len()
    }

    /// The game event types the mod asked for, by hash, 0 for every type.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.game_event_types().is_empty());
    /// ```
    #[must_use]
    pub fn game_event_types(&self) -> Vec<u32> {
        table::with(Vec::new(), |s| s.game_events.iter().map(|g| g.0).collect())
    }

    /// Hands the game event of `type_hash` with `bytes` to every handler of
    /// the mod for that type or for every type, on this thread; returns how
    /// many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.raise_game_event(0xd8ec_982a, &[0; 16]), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub fn raise_game_event(&self, type_hash: u32, bytes: &[u8]) -> usize {
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.game_events
                .iter()
                .filter(|g| g.0 == 0 || g.0 == type_hash)
                .map(|g| (g.1, g.2))
                .collect()
        });
        let data = crate::abi::GameEventData {
            header: Header::of::<crate::abi::GameEventData>(),
            type_hash,
            size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            data: bytes.as_ptr(),
        };
        for &(callback, user) in &handlers {
            // SAFETY: `data` lives for the call and `user` is the pointer the
            // mod subscribed with.
            unsafe {
                callback(
                    &raw const data,
                    core::ptr::with_exposed_provenance_mut(user),
                );
            };
        }
        handlers.len()
    }

    /// The mod events the mod defined, by name, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.defined_events().is_empty());
    /// ```
    #[must_use]
    pub fn defined_events(&self) -> Vec<String> {
        table::with(Vec::new(), |s| {
            s.defined_events.iter().map(|d| d.0.clone()).collect()
        })
    }

    /// The names of the mod events the mod sent, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.sent_events().is_empty());
    /// ```
    #[must_use]
    pub fn sent_events(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.sent_events.clone())
    }

    /// Sends the mod event `E`, as the mod that defines it would, to every
    /// handler of the mod whose layout `E` serves; returns how many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::event(name = "trading-post:sold")]
    /// #[derive(Clone, Copy, Debug)]
    /// pub struct Sold {
    ///     pub item: u32,
    /// }
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.send_mod_event(&Sold { item: 1 }), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub fn send_mod_event<E: crate::services::ModEvent>(&self, event: &E) -> usize {
        let offered: Vec<(&str, u64)> = E::LAYOUT
            .iter()
            // SAFETY: the declaration's names are static strings.
            .filter_map(|f| unsafe { f.name.as_str() }.map(|n| (n, f.hash)))
            .collect();
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.mod_event_subscriptions
                .iter()
                .filter(|sub| {
                    sub.0 == E::NAME
                        && crate::services::compare(&table::view(&sub.1), &offered).is_ok()
                })
                .map(|sub| (sub.2, sub.3))
                .collect()
        });
        for &(callback, user) in &handlers {
            // SAFETY: the layouts match, the event lives for the call, and
            // `user` is the pointer the mod subscribed with.
            unsafe {
                callback(
                    core::ptr::from_ref(event).cast(),
                    core::ptr::with_exposed_provenance_mut(user),
                );
            }
        }
        handlers.len()
    }
}
