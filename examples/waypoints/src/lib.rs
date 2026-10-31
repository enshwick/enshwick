//! Example: a join mod in two halves. The server half offers the player
//! actions "Mark my position" and "Clear my mark" and sends the result to
//! that player's client over a channel; the client half draws the mark on
//! the HUD until it is cleared.

use arc_swap::ArcSwapOption;
use enshwick::abi::Side;
use enshwick::actions::{Action, Outcome, Request};
use enshwick::i18n::{Language, Texts, language};
use enshwick::net::{Channel, Peer, SendError};
use enshwick::state::ModLocal;
use enshwick::ui::{Anchor, Hud, Label};
use enshwick::{Load, Mod, Off, PlayerKey};

// The keys of `src/texts.toml`, in English and Ukrainian.
enshwick::texts!();

/// What the server tells a player's client: the whole state each time, so a
/// message that arrives twice changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Waypoint {
    /// Show the mark at this place, in metres.
    Set([f64; 3]),
    /// Show no mark.
    Clear,
}

/// The channel both halves open, by its name and version.
const CHANNEL_NAME: &str = "waypoint";
/// The version of [`Waypoint`] on the channel; a change of the type raises it.
const CHANNEL_VERSION: u16 = 1;

static CHANNEL: ModLocal<Channel<Waypoint>> = ModLocal::new();

/// The mod's texts, for the client's HUD line.
static TEXTS: ModLocal<Texts> = ModLocal::new();

/// The mark the client shows, with its HUD line built once for the
/// language the line is in.
#[derive(Debug)]
struct Shown {
    at: [f64; 3],
    language: Language,
    line: String,
}

impl Shown {
    fn new(at: [f64; 3], language: Language) -> Self {
        let [x, y, z] = at.map(|metres| format!("{metres:.0}"));
        let values = [
            (texts::value::X, x.as_str()),
            (texts::value::Y, &y),
            (texts::value::Z, &z),
        ];
        let line = TEXTS
            .get()
            .map_or_else(String::new, |all| all.fill(texts::HUD, language, &values));
        Self { at, language, line }
    }
}

/// The mark the client shows: written by the channel's handler, read each
/// HUD frame without a lock or an allocation.
static SHOWN: ArcSwapOption<Shown> = ArcSwapOption::const_empty();

/// Takes what the server sent as the mark to show.
fn show(waypoint: Waypoint) {
    match waypoint {
        Waypoint::Set(at) => SHOWN.store(Some(Shown::new(at, language()).into())),
        Waypoint::Clear => SHOWN.store(None),
    }
}

/// Draws the mark, if any; builds its line again only when the player has
/// changed the game's language since.
fn draw(hud: &mut Hud<'_>) {
    let shown = SHOWN.load();
    let Some(mark) = shown.as_deref() else {
        return;
    };
    let now = language();
    if mark.language == now {
        hud.text(Anchor::TopRight, Label::new(&mark.line));
    } else {
        let again = Shown::new(mark.at, now);
        hud.text(Anchor::TopRight, Label::new(&again.line));
        SHOWN.store(Some(again.into()));
    }
}

/// Sends `waypoint` to `player`'s client.
fn tell(player: PlayerKey, waypoint: &Waypoint) -> Result<(), SendError> {
    CHANNEL.get().map_or(Err(SendError::NotReady), |channel| {
        channel.send(Peer::Player(player), waypoint)
    })
}

/// The action's answer: `done` in the player's language when the client got
/// the message, else why not.
fn answer(request: &Request<'_>, sent: Result<(), SendError>, done: &str) -> Outcome {
    match sent {
        Ok(()) => Outcome::done(request.say(done, &[])),
        Err(why) => {
            Outcome::refused(request.say(texts::NOT_SENT, &[(texts::value::WHY, &why.to_string())]))
        }
    }
}

/// The example waypoints mod.
#[derive(Debug)]
pub struct Waypoints;

impl Mod for Waypoints {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let all_texts = load.texts()?;
        let _first = TEXTS.set(all_texts.clone());
        // Both halves open the channel; only the client takes what comes in.
        let channel = load.channel(CHANNEL_NAME, CHANNEL_VERSION, |from, waypoint| {
            if from == Peer::Server {
                show(waypoint);
            }
        })?;
        let _first = CHANNEL.set(channel);

        // A client draws the mark; it also runs the server half while it
        // hosts a game.
        if load.host().build().side == Side::CLIENT {
            load.on_hud(draw)?;
        }

        let world = load.world()?;
        let actions = load.actions(all_texts)?;
        actions.add(Action::new(texts::MARK, texts::MARK), move |request| {
            let Some(at) = world.position(request.player.entity) else {
                return Outcome::refused(request.say(texts::NO_POSITION, &[]));
            };
            answer(
                request,
                tell(request.player.key, &Waypoint::Set(at)),
                texts::MARKED,
            )
        })?;
        actions.add(Action::new(texts::CLEAR, texts::CLEAR), |request| {
            answer(
                request,
                tell(request.player.key, &Waypoint::Clear),
                texts::CLEARED,
            )
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Waypoints);

#[cfg(test)]
mod tests {
    use enshwick::abi::{ActionCall, Header, PlayerInfo, Side, Status};
    use enshwick::content::EngineFacts;
    use enshwick::i18n::Texts;
    use enshwick::net::{Peer, SendError};
    use enshwick::test::{Drawn, FakeHost, facts};
    use enshwick::{Entity, PlayerKey};

    use super::{Waypoint, texts};

    #[test]
    fn every_text_exists_in_both_languages() {
        let texts = Texts::parse(texts::TOML).expect("the texts parse");
        assert!(
            texts.untranslated().is_empty(),
            "{:?}",
            texts.untranslated()
        );
    }

    #[test]
    fn the_actions_send_the_mark_and_its_clearing_and_need_a_position() {
        let player = PlayerKey::new(7);
        let character = Entity(2);
        let host = FakeHost::new().world().net_peer(Peer::Player(player));
        let transform = host
            .hash(facts::CURRENT_TRANSFORM.id())
            .expect("the preset has it");
        // A CurrentTransform starts with the position's fixed-point coordinates.
        let place: Vec<u8> = enshwick::ecs::fixed([4.0, 5.0, 6.0])
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let loader = host.component(character, transform, &place).install();
        // SAFETY: the exports of this crate's `declare_mod!(Waypoints)`.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(loader.draw_hud().is_empty(), "a server draws no HUD");

        let info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: character,
            key: player,
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        };
        let answer = loader.ask_actions(ActionCall::RUN, &info, 0, "mark", "");
        assert!(
            answer.as_ref().is_some_and(|text| text.contains("Marked.")),
            "{answer:?}"
        );
        let answer = loader.ask_actions(ActionCall::RUN, &info, 0, "clear", "");
        assert!(
            answer
                .as_ref()
                .is_some_and(|text| text.contains("Mark cleared.")),
            "{answer:?}"
        );
        let sent = loader.net_sent();
        assert_eq!(sent.len(), 2, "a mark, then its clearing");
        assert!(
            sent.iter()
                .all(|(name, to, _)| name == "waypoint@1" && *to == player)
        );

        let elsewhere = PlayerInfo {
            entity: Entity(3),
            ..info
        };
        let answer = loader.ask_actions(ActionCall::RUN, &elsewhere, 0, "mark", "");
        assert!(
            answer
                .as_ref()
                .is_some_and(|text| text.contains("Your position is not known yet.")),
            "{answer:?}"
        );
        assert_eq!(
            loader.net_sent().len(),
            2,
            "nothing sent without a position"
        );

        let stranger = PlayerKey::new(8);
        assert_eq!(
            super::tell(stranger, &Waypoint::Clear),
            Err(SendError::NotReady)
        );
    }

    #[test]
    fn the_client_half_draws_what_the_server_sent_until_it_is_cleared() {
        let loader = FakeHost::new().world().side(Side::CLIENT).install();
        // SAFETY: the exports of this crate's `declare_mod!(Waypoints)`.
        let status = unsafe { loader.start(super::enshwick_mod_init, super::enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(loader.draw_hud().is_empty());

        assert!(loader.net_deliver(
            "waypoint@1",
            Peer::Server,
            &Waypoint::Set([12.4, 30.0, -7.6])
        ));
        let drawn = loader.draw_hud();
        assert!(
            matches!(drawn.as_slice(), [(_, Drawn::Label { text, .. })] if text == "Mark at 12, 30, -8"),
            "{drawn:?}"
        );

        assert!(loader.net_deliver("waypoint@1", Peer::Server, &Waypoint::Clear));
        assert!(loader.draw_hud().is_empty());
    }
}
