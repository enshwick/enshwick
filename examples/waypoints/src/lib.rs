//! Example: a join mod in two halves. The server half offers the player
//! actions "Mark my position" and "Clear my mark" and sends the result to
//! that player's client over a channel; the client half draws the mark on
//! the HUD until it is cleared.

use arc_swap::ArcSwapOption;
use enshwick::abi::Side;
use enshwick::actions::{Action, Outcome, Request};
use enshwick::i18n::language;
use enshwick::net::{Channel, Peer, SendError};
use enshwick::state::ModLocal;
use enshwick::ui::{Anchor, Label};
use enshwick::{Load, Mod, Off, PlayerKey};

/// The mod's texts, in English and Ukrainian.
const TEXTS: &str = include_str!("texts.toml");

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

/// The mark the client shows: written by the channel's handler, read each
/// HUD frame without a lock.
static SHOWN: ArcSwapOption<[f64; 3]> = ArcSwapOption::const_empty();

/// Takes what the server sent as the mark to show.
fn show(waypoint: Waypoint) {
    match waypoint {
        Waypoint::Set(at) => SHOWN.store(Some(at.into())),
        Waypoint::Clear => SHOWN.store(None),
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
        Err(why) => Outcome::refused(request.say("not-sent", &[("why", &why.to_string())])),
    }
}

/// The example waypoints mod.
#[derive(Debug)]
pub struct Waypoints;

impl Mod for Waypoints {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = load.texts(TEXTS)?;
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
            let hud_texts = texts.clone();
            load.on_hud(move |hud| {
                if let Some(at) = SHOWN.load().as_deref() {
                    let [x, y, z] = at.map(|metres| format!("{metres:.0}"));
                    let line =
                        hud_texts.fill("hud", language(), &[("x", &x), ("y", &y), ("z", &z)]);
                    hud.text(Anchor::TopRight, Label::new(&line));
                }
            })?;
        }

        let world = load.world()?;
        let actions = load.actions(texts)?;
        actions.add(Action::new("mark", "mark"), move |request| {
            let Some(at) = world.position(request.player.entity) else {
                return Outcome::refused(request.say("no-position", &[]));
            };
            answer(
                request,
                tell(request.player.key, &Waypoint::Set(at)),
                "marked",
            )
        })?;
        actions.add(Action::new("clear", "clear"), |request| {
            answer(
                request,
                tell(request.player.key, &Waypoint::Clear),
                "cleared",
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

    use super::{TEXTS, Waypoint};

    #[test]
    fn every_text_exists_in_both_languages() {
        let texts = Texts::parse(TEXTS).expect("the texts parse");
        assert!(
            texts.untranslated().is_empty(),
            "{:?}",
            texts.untranslated()
        );
    }

    #[test]
    fn the_mark_action_sends_the_player_s_position_to_their_client() {
        let player = PlayerKey::new(7);
        let character = Entity(2);
        let host = FakeHost::new().world().net_peer(Peer::Player(player));
        let transform = host
            .hash(facts::CURRENT_TRANSFORM.id())
            .expect("the preset has it");
        // A CurrentTransform starts with three fixed-point coordinates, 32
        // fractional bits each: the character stands at 4, 5, 6.
        let place: Vec<u8> = [4_i64 << 32, 5 << 32, 6 << 32]
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
        let sent = loader.net_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0.as_str(), sent[0].1), ("waypoint@1", player));

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
