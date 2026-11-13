//! The host the mod runs in: its side, whether it hosts a game, the other
//! mods and their exports and services, the mod's features and status, and
//! its messages.

use enshwick_core::abi::Header;
use enshwick_core::abi::ModInfo;
use enshwick_core::abi::Side;
use enshwick_core::abi::Str;

use crate::test::{FakeHost, Installed, table};

impl FakeHost {
    /// The side the game runs as.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Whether a client hosts a game now; the dedicated server always runs
    /// one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new().side(Side::CLIENT).hosting(true).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn hosting(mut self, hosting: bool) -> Self {
        self.hosting = hosting;
        self
    }

    /// The account of the hosted game's host, whom account actions never
    /// kick or ban.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::Side;
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .side(Side::CLIENT)
    ///     .hosting(true)
    ///     .host_key(enshwick::PlayerKey::new(7))
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn host_key(mut self, key: enshwick_core::PlayerKey) -> Self {
        self.host_key = Some(key);
        self
    }

    /// The entity world of the running server, which a system context of
    /// that world holds; 0 for none.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().server_world(0x2000).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn server_world(mut self, world: u64) -> Self {
        self.server_world = world;
        self
    }

    /// A peer that takes mod messages: a player's account key on the
    /// server, or [`Peer::Server`](enshwick_core::net::Peer::Server) on a client.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::net::Peer;
    ///
    /// let loader = enshwick::test::FakeHost::new().net_peer(Peer::Player(enshwick::PlayerKey::new(7))).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn net_peer(mut self, peer: enshwick_core::net::Peer) -> Self {
        let raw = match peer {
            enshwick_core::net::Peer::Server => enshwick_core::abi::NetMessage::SERVER,
            enshwick_core::net::Peer::Player(key) => key,
        };
        self.net_peers.insert(raw.get());
        self
    }

    /// A content id, `<mod>:<name>`, that another loaded mod declared, as
    /// for an item of its own the mod under test names by GUID.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().declared_content("you.lockers:locker").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn declared_content(mut self, id: &str) -> Self {
        self.declared_content.push(id.to_owned());
        self
    }

    /// A loaded mod `enshwick::mods::all()` lists, in order; the loader
    /// itself comes first in the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, ManifestFlags, ModInfo, Side, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .loaded(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("enshwick"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         flags: ManifestFlags::NONE,
    ///         off: 0,
    ///         file: Str::new("mods/enshwick/enshwick.dll"),
    ///         ..ModInfo::EMPTY
    ///     })
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn loaded(mut self, info: ModInfo) -> Self {
        self.mods.push(info);
        self
    }

    /// Another mod `id` that exports only `names`, each `<kind>:<local
    /// name>` as a manifest lists it (`call:price-of`, `content:locker`): as
    /// the loader, the fake refuses the mod a call, link, service, event,
    /// content id or stat of `id` outside them, matching kind and name.
    /// Without it, the fake takes every name of another mod as exported,
    /// where the loader refuses every name a mod does not export: a test
    /// that links another mod passes that mod's exports here, so it fails
    /// where the game would.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .exports("you.trading-post", &["call:price-of", "event:sold"])
    ///     .install();
    /// # let _ = loader;
    /// ```
    ///
    /// # Panics
    ///
    /// When a name lacks its kind, as the loader refuses a manifest whose
    /// export lacks it.
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "a test that hands the fake a bare name fails as the loader refuses it"
    )]
    pub fn exports(mut self, id: &str, names: &[&str]) -> Self {
        if let Some(bare) = enshwick_core::__fake::rules::bad_export(names) {
            panic!("the export {bare:?} lacks its kind or its local name: write <kind>:<name>");
        }
        self.exports.insert(
            id.to_owned(),
            names.iter().map(|&name| name.to_owned()).collect(),
        );
        self
    }

    /// A server mod the last refused join found missing, which
    /// `enshwick::mods::join_missing()` lists, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Header, ModInfo, Side, Str};
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .join_missing(ModInfo {
    ///         header: Header::of::<ModInfo>(),
    ///         id: Str::new("you.trading-post"),
    ///         version: Str::new("0.2.0"),
    ///         side: Side::BOTH,
    ///         off: 1,
    ///         ..ModInfo::EMPTY
    ///     })
    ///     .install();
    /// // SAFETY: the installed fake's table.
    /// let api = unsafe { &*loader.api() };
    /// let mut count = 0;
    /// let size = u32::try_from(size_of::<ModInfo>())?;
    /// let status = (api.join_missing)(loader.handle(), core::ptr::null_mut(), size, 0, &mut count);
    /// assert!(status.is_ok());
    /// assert_eq!(count, 1);
    /// # Ok::<(), core::num::TryFromIntError>(())
    /// ```
    #[must_use]
    pub fn join_missing(mut self, info: ModInfo) -> Self {
        self.join_missing.push(info);
        self
    }

    /// A service another mod publishes.
    ///
    /// # Examples
    ///
    /// ```
    /// #[enshwick::service(of = trading_post, name = "prices")]
    /// pub struct Prices {
    ///     pub price: extern "system" fn(item: u32) -> u32,
    /// }
    ///
    /// extern "system" fn price(_item: u32) -> u32 {
    ///     3
    /// }
    ///
    /// static PRICES: Prices = Prices { price };
    ///
    /// let loader = enshwick::test::FakeHost::new().service(&PRICES).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn service<T: enshwick_core::services::Service>(mut self, table: &'static T) -> Self {
        self.services.push(FakeService {
            name: T::NAME.to_owned(),
            major: T::MAJOR,
            fields: T::LAYOUT
                .iter()
                // SAFETY: the declaration's names are static strings.
                .filter_map(|f| unsafe { f.name.as_str() }.map(|n| (n.to_owned(), f.hash)))
                .collect(),
            table: core::ptr::from_ref(table).expose_provenance(),
        });
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The mod's features as it last reported them with
    /// [`Load::feature`](enshwick_core::Load::feature), in the order it first named
    /// them: the name and, for one that is off, the reason.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.features().is_empty());
    /// ```
    #[must_use]
    pub fn features(&self) -> Vec<(String, Option<String>)> {
        table::with(Vec::new(), |s| s.features.clone())
    }

    /// The lines the mod's status sources give now, each source's text in
    /// registration order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.status().is_empty());
    /// ```
    #[must_use]
    pub fn status(&self) -> Vec<String> {
        let sources = table::with(Vec::new(), |s| s.status_sources.clone());
        sources
            .into_iter()
            .map(|source| {
                let mut out = Str::EMPTY;
                // SAFETY: `user` is the pointer the mod registered with; a
                // writable out pointer.
                source.run(|callback, user| unsafe { callback(user, &raw mut out) });
                // SAFETY: the text stays valid until the source runs again.
                unsafe { out.as_str() }.unwrap_or("").to_owned()
            })
            .collect()
    }

    /// The messages the mod sent: the channel's full name (`name@version`),
    /// the raw peer ([`NetMessage::SERVER`](enshwick_core::abi::NetMessage::SERVER),
    /// a player's key, or 0 for every client) and the encoded bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.net_sent().is_empty());
    /// ```
    #[must_use]
    pub fn net_sent(&self) -> Vec<(String, enshwick_core::PlayerKey, Vec<u8>)> {
        table::with(Vec::new(), |s| {
            s.net_sent
                .iter()
                .map(|(id, peer, bytes)| {
                    let name = s
                        .channels
                        .iter()
                        .find(|c| c.1 == *id)
                        .map_or_else(String::new, |c| c.0.clone());
                    (name, *peer, bytes.clone())
                })
                .collect()
        })
    }

    /// Hands `message` from `from` to the mod's channel `name@version`, as
    /// the loader does when one comes in, encoded as the channel encodes
    /// it; false when the mod opened no such channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::net::Peer;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.net_deliver("markers@1", Peer::Server, &5u32));
    /// ```
    #[must_use]
    pub fn net_deliver<M: serde::Serialize>(
        &self,
        name: &str,
        from: enshwick_core::net::Peer,
        message: &M,
    ) -> bool {
        let Ok(bytes) = postcard::to_allocvec(message) else {
            return false;
        };
        let channel = table::with(None, |s| {
            s.channels.iter().find(|c| c.0 == name).map(|c| (c.1, c.2))
        });
        let Some((id, handler)) = channel else {
            return false;
        };
        let peer = match from {
            enshwick_core::net::Peer::Server => enshwick_core::abi::NetMessage::SERVER,
            enshwick_core::net::Peer::Player(key) => key,
        };
        let raw = enshwick_core::abi::NetMessage {
            header: Header::of::<enshwick_core::abi::NetMessage>(),
            channel: id,
            reserved: 0,
            peer,
            data: bytes.as_ptr(),
            size: u32::try_from(bytes.len()).unwrap_or(0),
            reserved_tail: 0,
        };
        // SAFETY: a handler the mod registered with its own pointer.
        handler.run(|callback, user| unsafe { callback(&raw const raw, user) });
        true
    }
}

/// A published service, as the fake keeps it.
#[derive(Clone, Debug)]
pub(crate) struct FakeService {
    pub(crate) name: String,
    pub(crate) major: u32,
    pub(crate) fields: Vec<(String, u64)>,
    pub(crate) table: usize,
}
