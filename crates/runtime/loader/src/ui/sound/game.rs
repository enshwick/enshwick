//! The client's sound system as the mods' sounds reach it: its start, stop
//! and instance calls, and its scenes.
//!
//! The interface's context holds the sound object `ui.sound` plays
//! through: the sound system and the `Ui` scene (client 26430625). The
//! `GameUi` that holds that object also keeps a pointer into the client
//! object, beside which the client keeps every scene's id in an array by
//! `SoundSceneType`, made once at the client's start. The loader reads the
//! other scenes there once, and takes them only when the `Ui` scene it
//! reads there is the interface's own.

use std::sync::OnceLock;

use enshwick_gamedata::catalogue;

use super::drain::{Mixer, Scene};
use crate::engine::loader::Loader;
use crate::engine::pinned;

type StartFn = unsafe extern "system" fn(
    system: usize,
    unused: usize,
    scene: u32,
    container: *const [u8; 16],
    params: *const PlayParams,
) -> u32;
type StopFn = unsafe extern "system" fn(system: usize, id: u32, fade_ns: i64);
type VolumeFn = unsafe extern "system" fn(system: usize, id: u32, volume: f32, fade_ns: i64);
type PositionFn = unsafe extern "system" fn(system: usize, id: u32, at: *const [i64; 3]);
type PlayingFn = unsafe extern "system" fn(system: usize, id: u32) -> u8;

/// The start's parameters, 0x28 bytes (client 26430625, `0x106ee50`): a
/// word the game's callers leave 0, the position in 32.32 metres, the
/// volume and two flag bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct PlayParams {
    unused: u64,
    at: [i64; 3],
    volume: f32,
    flag: u8,
    flags: u8,
    padding: [u8; 2],
}

const _: () = assert!(size_of::<PlayParams>() == 0x28);

/// The flag `ui.sound` sets for a sound from no place (client 26430625,
/// `0xa4f474`), which the game's own sounds of the local player set too.
const UNPLACED: u8 = 0x04;

/// The functions and reads the sounds take from the build.
#[derive(Debug)]
pub(crate) struct Calls {
    start: StartFn,
    stop: StopFn,
    volume: VolumeFn,
    position: PositionFn,
    playing: PlayingFn,
    ui_object: usize,
    ui_system: usize,
    ui_scene: usize,
    /// Where the sound object holds its pointer into the client object.
    link: usize,
    client_link: usize,
    client_ui_scene: usize,
    /// The index of each scene in the client's array.
    music: usize,
    ui: usize,
    game: usize,
    /// The scenes the client made at its start, kept by [`Calls::scenes`]:
    /// the music's and the world's, 0 for one not reached.
    scenes: OnceLock<(u32, u32)>,
}

/// The table ids the sounds need.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::sound::START.id(),
    pinned::sound::STOP.id(),
    pinned::sound::SET_VOLUME.id(),
    pinned::sound::SET_POSITION.id(),
    pinned::sound::IS_PLAYING.id(),
    pinned::sound::UI_OBJECT.id(),
    pinned::sound::UI_SYSTEM.id(),
    pinned::sound::UI_SCENE.id(),
    pinned::sound::GAMEUI_LINK.id(),
    pinned::sound::GAMEUI_UI_SCENE.id(),
    pinned::sound::CLIENT_LINK.id(),
    pinned::sound::CLIENT_UI_SCENE.id(),
];

impl Calls {
    /// The calls on `loader`'s build.
    ///
    /// # Errors
    ///
    /// The id of the first entry the build lacks.
    pub(crate) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let at = |id: pinned::Symbol| {
            loader
                .address(id)
                .and_then(|address| usize::try_from(address).ok())
                .ok_or(id.id())
        };
        let offset = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
        let scene = |id: &'static str| {
            catalogue::constant(id, &loader.reflection)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(id)
        };
        let ui_scene = offset(pinned::sound::UI_SCENE)?;
        // The GameUi copies the link and the `Ui` scene side by side; the
        // sound object starts where its `Ui` scene lands minus `ui_scene`.
        let link = (offset(pinned::sound::GAMEUI_LINK)? + ui_scene)
            .checked_sub(offset(pinned::sound::GAMEUI_UI_SCENE)?)
            .ok_or(pinned::sound::GAMEUI_LINK.id())?;
        let start = at(pinned::sound::START)?;
        let stop = at(pinned::sound::STOP)?;
        let volume = at(pinned::sound::SET_VOLUME)?;
        let position = at(pinned::sound::SET_POSITION)?;
        let playing = at(pinned::sound::IS_PLAYING)?;
        // SAFETY: the routed `sound.start`, whose route checked its first
        // instructions on this executable (`anchors/routes.toml`), with the
        // signature the decompiler gives on client 26430625.
        let start = unsafe { core::mem::transmute::<usize, StartFn>(start) };
        // SAFETY: the routed `sound.stop`, checked by its route, with the
        // decompiler's signature on client 26430625.
        let stop = unsafe { core::mem::transmute::<usize, StopFn>(stop) };
        // SAFETY: the routed `sound.set_volume`, checked by its route, with
        // the decompiler's signature on client 26430625.
        let volume = unsafe { core::mem::transmute::<usize, VolumeFn>(volume) };
        // SAFETY: the routed `sound.set_position`, checked by its route,
        // with the decompiler's signature on client 26430625.
        let position = unsafe { core::mem::transmute::<usize, PositionFn>(position) };
        // SAFETY: the routed `sound.is_playing`, checked by its route, with
        // the decompiler's signature on client 26430625.
        let playing = unsafe { core::mem::transmute::<usize, PlayingFn>(playing) };
        Ok(Self {
            start,
            stop,
            volume,
            position,
            playing,
            ui_object: offset(pinned::sound::UI_OBJECT)?,
            ui_system: offset(pinned::sound::UI_SYSTEM)?,
            ui_scene,
            link,
            client_link: offset(pinned::sound::CLIENT_LINK)?,
            client_ui_scene: offset(pinned::sound::CLIENT_UI_SCENE)?,
            music: scene(catalogue::ids::SOUND_SCENE_MUSIC)?,
            ui: scene(catalogue::ids::SOUND_SCENE_UI)?,
            game: scene(catalogue::ids::SOUND_SCENE_GAME)?,
            scenes: OnceLock::new(),
        })
    }

    /// The music's and the world's scenes, 0 for one not reached: kept once
    /// a read finds them or finds the client's `Ui` scene another, and read
    /// again at the next frame while the client has not made them yet.
    fn scenes(&self, object: usize, ui: u32) -> (u32, u32) {
        if let Some(&scenes) = self.scenes.get() {
            return scenes;
        }
        self.client_scenes(object, ui)
            .map_or((0, 0), |scenes| *self.scenes.get_or_init(|| scenes))
    }

    /// The scenes past `Ui` the client object holds, read with checked
    /// reads from the sound object at `object` whose `Ui` scene is `ui`:
    /// `None` while a read fails or the client's `Ui` scene is still 0, and
    /// none reached when it is another scene.
    fn client_scenes(&self, object: usize, ui: u32) -> Option<(u32, u32)> {
        let link =
            usize::from_le_bytes(crate::engine::pages::read(object.checked_add(self.link)?)?);
        let client = link.checked_sub(self.client_link)?;
        let base = client
            .checked_add(self.client_ui_scene)?
            .checked_sub(self.ui * 4)?;
        let read = |index: usize| {
            crate::engine::pages::read(base.checked_add(index * 4)?).map(u32::from_le_bytes)
        };
        let client_ui = read(self.ui)?;
        if client_ui == 0 {
            return None;
        }
        if client_ui != ui {
            crate::log!(
                WARN,
                "sound: the client's Ui scene {client_ui:#x} is not the interface's {ui:#x}; music and world sounds are off"
            );
            return Some((0, 0));
        }
        let (music, game) = (read(self.music)?, read(self.game)?);
        crate::log!(
            INFO,
            "sound: scenes Ui {ui:#x}, Music {music:#x}, Game {game:#x} reached"
        );
        Some((music, game))
    }
}

/// The sound system as one frame reaches it.
pub(crate) struct GameMixer<'a> {
    calls: &'a Calls,
    system: usize,
    ui: u32,
    music: u32,
    game: u32,
}

impl<'a> GameMixer<'a> {
    /// The sound system of the interface's `context`; `None` while it holds
    /// none, as before the game made it.
    ///
    /// # Safety
    ///
    /// - Called inside the interface frame on its thread, with `context` the
    ///   frame's own live, non-zero context.
    pub(crate) unsafe fn of(calls: &'a Calls, context: usize) -> Option<Self> {
        // SAFETY: the caller's live context, whose sound object pointer sits
        // at the routed `sound.ui_object` (`ui.sound` reads it there).
        let object = unsafe { crate::engine::memory::read::<usize>(context, calls.ui_object) };
        if object == 0 {
            return None;
        }
        // SAFETY: `object` is the context's non-null sound object, whose
        // system sits at the routed offset `ui.sound` reads it at.
        let system = unsafe { crate::engine::memory::read::<usize>(object, calls.ui_system) };
        // SAFETY: the same sound object, whose `Ui` scene sits at the routed
        // offset `ui.sound` reads it at.
        let ui = unsafe { crate::engine::memory::read::<u32>(object, calls.ui_scene) };
        if system == 0 || ui == 0 {
            return None;
        }
        let (music, game) = calls.scenes(object, ui);
        Some(Self {
            calls,
            system,
            ui,
            music,
            game,
        })
    }

    fn scene(&self, scene: Scene) -> u32 {
        match scene {
            Scene::Ui => self.ui,
            Scene::Game => self.game,
            Scene::Music => self.music,
        }
    }
}

/// `ms` as the game's nanoseconds.
fn nanos(ms: u32) -> i64 {
    i64::from(ms) * 1_000_000
}

impl Mixer for GameMixer<'_> {
    fn reaches(&self, scene: Scene) -> bool {
        self.scene(scene) != 0
    }

    fn start(
        &mut self,
        scene: Scene,
        container: [u8; 16],
        at: Option<[i64; 3]>,
        volume: f32,
    ) -> Option<u32> {
        let params = PlayParams {
            unused: 0,
            at: at.unwrap_or_default(),
            volume,
            flag: 0,
            flags: if at.is_some() { 0 } else { UNPLACED },
            padding: [0; 2],
        };
        // SAFETY: the routed start on the frame's live system, on the
        // interface thread where the game's own `ui.sound` starts sounds;
        // both pointers are to locals alive for the call, and the start
        // reads the GUID only as a key (client 26430625).
        let id = unsafe {
            (self.calls.start)(
                self.system,
                0,
                self.scene(scene),
                &raw const container,
                &raw const params,
            )
        };
        (id != 0).then_some(id)
    }

    fn stop(&mut self, id: u32, fade_ms: u32) {
        // SAFETY: the routed stop on the frame's live system, on its thread;
        // an id of no instance finds nothing (the generation check of
        // `0x105b580`, client 26430625).
        unsafe { (self.calls.stop)(self.system, id, nanos(fade_ms)) };
    }

    fn volume(&mut self, id: u32, volume: f32, fade_ms: u32) {
        // SAFETY: as `stop`; the game clamps the volume to 0 to 1.
        unsafe { (self.calls.volume)(self.system, id, volume, nanos(fade_ms)) };
    }

    fn position(&mut self, id: u32, at: [i64; 3]) {
        // SAFETY: as `stop`; `at` is a local alive for the call, read as 24
        // bytes.
        unsafe { (self.calls.position)(self.system, id, &raw const at) };
    }

    fn playing(&mut self, id: u32) -> bool {
        // SAFETY: as `stop`; the call only looks the id up.
        unsafe { (self.calls.playing)(self.system, id) != 0 }
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    unsafe extern "system" fn start(
        _: usize,
        _: usize,
        _: u32,
        _: *const [u8; 16],
        _: *const PlayParams,
    ) -> u32 {
        0
    }
    unsafe extern "system" fn stop(_: usize, _: u32, _: i64) {}
    unsafe extern "system" fn volume(_: usize, _: u32, _: f32, _: i64) {}
    unsafe extern "system" fn position(_: usize, _: u32, _: *const [i64; 3]) {}
    unsafe extern "system" fn playing(_: usize, _: u32) -> u8 {
        0
    }

    /// Calls whose sound object links its client at its second word, and
    /// whose client keeps the Music, Ui and Game scenes in its first three.
    fn calls() -> Calls {
        Calls {
            start,
            stop,
            volume,
            position,
            playing,
            ui_object: 0,
            ui_system: 0,
            ui_scene: 0,
            link: size_of::<usize>(),
            client_link: 0,
            client_ui_scene: 4,
            music: 0,
            ui: 1,
            game: 2,
            scenes: OnceLock::new(),
        }
    }

    const UI: u32 = 7;

    /// Reads the scenes of `client` through a sound object that links it.
    fn scenes(calls: &Calls, client: &[AtomicU32; 3]) -> (u32, u32) {
        let object = [0, client.as_ptr().expose_provenance()];
        calls.scenes(object.as_ptr().expose_provenance(), UI)
    }

    #[test]
    fn the_scenes_are_read_again_until_the_client_made_them_and_then_kept() {
        let calls = calls();
        let client = [const { AtomicU32::new(0) }; 3];
        assert_eq!(scenes(&calls, &client), (0, 0));
        for (cell, scene) in client.iter().zip([5, UI, 9]) {
            cell.store(scene, Ordering::Relaxed);
        }
        assert_eq!(scenes(&calls, &client), (5, 9));
        client[2].store(11, Ordering::Relaxed);
        assert_eq!(scenes(&calls, &client), (5, 9), "kept from the first read");
    }

    #[test]
    fn a_client_ui_scene_that_is_not_the_interface_s_reaches_no_scene_for_good() {
        let calls = calls();
        let client = [5, UI + 1, 9].map(AtomicU32::new);
        assert_eq!(scenes(&calls, &client), (0, 0));
        client[1].store(UI, Ordering::Relaxed);
        assert_eq!(scenes(&calls, &client), (0, 0));
    }
}
