//! The game's lore categories, for a [`NewJournalPage`](super::NewJournalPage).
//!
//! Each is a card of the journal's world information screen and the text
//! tag a page names as its `category`. The game ships no name for a
//! category, only its card's texts, so each name here is enshwick's own,
//! from the card's English text on client archive 1287369. A tag under
//! which the running game's journal lists no page is refused like any other.
//!
//! ```
//! # extern crate enshwick_core as enshwick;
//! use enshwick::content::journal::lore_category;
//!
//! assert!(lore_category::ALL.contains(&lore_category::GENERAL));
//! ```

use crate::content::Guid;

/// The card "General"; the name is enshwick's, from that text (client archive 1287369).
pub const GENERAL: Guid = Guid::parse("f94a5ece-d3fb-4f32-8d6e-e72bdffaba39");
/// The card "Ancients and the Flame"; the name is enshwick's, from that text (client archive 1287369).
pub const ANCIENTS_AND_THE_FLAME: Guid = Guid::parse("a86a5728-f02a-4e28-b0f5-1d53d69f7261");
/// The card "The Shroud, Elixir, and Elixir Wells"; the name is enshwick's, from that text (client archive 1287369).
pub const THE_SHROUD_ELIXIR_AND_ELIXIR_WELLS: Guid =
    Guid::parse("8ab85ee5-5709-4ff7-b5cf-d60954cafec0");
/// The card "About Scavengers"; the name is enshwick's, from that text (client archive 1287369).
pub const ABOUT_SCAVENGERS: Guid = Guid::parse("04123d11-4918-4fd9-853f-682e39ee8b50");
/// The card "Vukah and Wildlife"; the name is enshwick's, from that text (client archive 1287369).
pub const VUKAH_AND_WILDLIFE: Guid = Guid::parse("e1a327ed-419c-4cef-9ccf-0d8bf142db0a");
/// The card "Nobles and History"; the name is enshwick's, from that text (client archive 1287369).
pub const NOBLES_AND_HISTORY: Guid = Guid::parse("58b2bc1e-713e-4b0a-9225-71df283e1c4f");
/// The card "Revelwoods"; the name is enshwick's, from that text (client archive 1287369).
pub const REVELWOODS: Guid = Guid::parse("771bb6b6-3ce5-4bfc-bb1a-7e1a1a2c9d2b");
/// The card "Nomad Highlands"; the name is enshwick's, from that text (client archive 1287369).
pub const NOMAD_HIGHLANDS: Guid = Guid::parse("2e66d970-04a3-4078-b3bf-3e33d4996728");
/// The card "Kindlewastes"; the name is enshwick's, from that text (client archive 1287369).
pub const KINDLEWASTES: Guid = Guid::parse("a57cfea9-74fc-42d0-8a12-b05a2ed112b9");
/// The card "The Hollow"; the name is enshwick's, from that text (client archive 1287369).
pub const THE_HOLLOW: Guid = Guid::parse("1e15a1ec-ecc1-4f6c-afb2-9eb3609dbdac");
/// The card "Cold Heights"; the name is enshwick's, from that text (client archive 1287369).
pub const COLD_HEIGHTS: Guid = Guid::parse("c811b8f1-bd2b-4e31-80a3-65a54ff794d9");
/// The card "Veilwater Basin"; the name is enshwick's, from that text (client archive 1287369).
pub const VEILWATER_BASIN: Guid = Guid::parse("437ca83f-c2ec-4feb-9095-108c627a6ac7");
/// The card "Drak"; the name is enshwick's, from that text (client archive 1287369).
pub const DRAK: Guid = Guid::parse("99e69e04-0fbc-4317-a9e3-1623261a4c0e");

/// Every category named here, in the order of the game's cards. The journal
/// of the running game decides which a page may take.
pub const ALL: &[Guid] = &[
    GENERAL,
    ANCIENTS_AND_THE_FLAME,
    THE_SHROUD_ELIXIR_AND_ELIXIR_WELLS,
    ABOUT_SCAVENGERS,
    VUKAH_AND_WILDLIFE,
    NOBLES_AND_HISTORY,
    REVELWOODS,
    NOMAD_HIGHLANDS,
    KINDLEWASTES,
    THE_HOLLOW,
    COLD_HEIGHTS,
    VEILWATER_BASIN,
    DRAK,
];
