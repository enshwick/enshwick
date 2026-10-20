//! The server's game settings: the difficulty and gameplay values its
//! configuration or the host's menu chose, read and changed while the world
//! runs.
//!
//! The server keeps one live `GameSettings` and every system reads it when
//! it runs, so a change takes effect on the next step. A change goes the
//! game's own way, as its admin command does: the loader copies the new
//! values in and raises `GameSettingsChangedEvent`, which clients copy into
//! their own world, and it also marks the settings for the server's next
//! info message, which every client takes too. The game validates no
//! change, so the loader refuses day and night lengths of zero or less,
//! which the clock divides by. Nothing writes a change to disk: the
//! configuration's values return with the next start.

use std::sync::OnceLock;

use enshwick_loader_api::abi::Status;

use crate::host::Loader;

/// The preset the game names custom settings with.
const CUSTOM: u8 = 4;

/// Where the server keeps its settings, from the build table and
/// reflection.
#[derive(Debug)]
struct Layout {
    settings: usize,
    preset: usize,
    resend: usize,
    size: usize,
    day_length: usize,
    night_length: usize,
    event: u32,
    event_size: usize,
    event_settings: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let offset = |ty: &str, field: &str| {
                reflection
                    .field_offset(ty, field)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let size_of = |ty: &str| {
                reflection
                    .type_info(ty)
                    .ok()
                    .and_then(|info| usize::try_from(info.size()).ok())
            };
            let settings = "keen::ecs::GameSettings";
            let event = "keen::ecs::GameSettingsChangedEvent";
            let event_info = reflection.type_info(event).ok()?;
            Some(Layout {
                settings: loader.offset("server.game_settings")?,
                preset: loader.offset("server.game_settings_preset")?,
                resend: loader.offset("server.game_settings_resend")?,
                size: size_of(settings)?,
                day_length: offset(settings, "dayTimeDuration")?,
                night_length: offset(settings, "nightTimeDuration")?,
                event: crate::world::type_hash(loader, event_info.va())?,
                event_size: usize::try_from(event_info.size()).ok()?,
                event_settings: offset(event, "newSettings")?,
            })
        })
        .as_ref()
}

/// The live settings, on the game thread.
fn live(loader: &Loader) -> Result<(&'static Layout, usize, usize), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    // SAFETY: the server holds the pointer to its settings from its start.
    let settings: usize = unsafe { crate::world::read(server, layout.settings) };
    if settings == 0 {
        return Err(Status::MISSING);
    }
    Ok((layout, server, settings))
}

/// Copies the live settings into `out`, which is their reflected size, and
/// returns the preset.
pub(crate) fn read(loader: &Loader, out: &mut [u8]) -> Result<u8, Status> {
    let (layout, server, settings) = live(loader)?;
    if out.len() != layout.size {
        return Err(Status::BAD_ARGS);
    }
    // SAFETY: the live settings span their reflected size, on the game
    // thread that alone writes them.
    unsafe {
        core::ptr::copy_nonoverlapping(
            core::ptr::with_exposed_provenance::<u8>(settings),
            out.as_mut_ptr(),
            out.len(),
        );
    };
    // SAFETY: the server holds its preset byte.
    Ok(unsafe { crate::world::read(server, layout.preset) })
}

/// Replaces the live settings with `bytes`, their whole reflected layout,
/// and tells the clients.
pub(crate) fn write(loader: &Loader, bytes: &[u8]) -> Result<(), Status> {
    let (layout, server, settings) = live(loader)?;
    if bytes.len() != layout.size {
        return Err(Status::BAD_ARGS);
    }
    let length = |at: usize| {
        bytes
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map_or(0, i64::from_le_bytes)
    };
    if length(layout.day_length) <= 0 || length(layout.night_length) <= 0 {
        return Err(Status::BAD_ARGS);
    }
    // SAFETY: as for `read`; the game's admin command writes them the same
    // way, between the systems that read them.
    unsafe {
        core::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            core::ptr::with_exposed_provenance_mut::<u8>(settings),
            bytes.len(),
        );
    };
    // SAFETY: the server's preset byte, on its game thread.
    unsafe { core::ptr::with_exposed_provenance_mut::<u8>(server + layout.preset).write(CUSTOM) };
    // SAFETY: the server's flag for the settings in its next info message,
    // on its game thread.
    unsafe { core::ptr::with_exposed_provenance_mut::<u8>(server + layout.resend).write(1) };
    let mut event = vec![0u8; layout.event_size];
    if let Some(out) = event.get_mut(layout.event_settings..layout.event_settings + bytes.len()) {
        out.copy_from_slice(bytes);
        if let Err(status) = crate::emits::push(loader, layout.event, &event) {
            crate::log!(
                DEBUG,
                "game settings changed; the event was refused ({}), the info message carries them",
                status.0
            );
        }
    }
    Ok(())
}
