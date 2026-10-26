//! Every struct that starts with a [`Header`] crosses the boundary only as far
//! as both sides know it, so a mod built against an older minor stays safe.

use super::*;

/// The fill of a value's own bytes.
const VALUE: u8 = 0xAB;
/// The fill of the bytes a mod owns past its struct.
const GUARD: u8 = 0x11;

/// Every struct of the ABI that starts with a [`Header`]; the test below
/// holds this list to `abi.rs`.
macro_rules! headed {
    ($check:ident) => {
        $check!(
            ActionCall,
            ActiveBuff,
            AffixDefData,
            AffixRoll,
            AltarInfo,
            BedCheck,
            BuffRequest,
            BuffTypeInfo,
            BuildCheck,
            BuildInfo,
            BuildingData,
            ChunkData,
            Comfort,
            CraftCheck,
            DamageCheck,
            DaytimeInfo,
            EventSpec,
            ExperiencePass,
            GameEventData,
            HostApi,
            Interaction,
            InventoryRequest,
            InventoryResult,
            ItemData,
            KnowledgeData,
            KnowledgeInfo,
            KnowledgeInput,
            Manifest,
            MigrationSet,
            ModInfo,
            NetMessage,
            PlayerData,
            PlayerDeathData,
            PlayerInfo,
            PointHandle,
            PointLink,
            PointSpec,
            QuestData,
            RenameCall,
            ServiceFound,
            ServiceSpec,
            SkillCheck,
            SpawnRequest,
            StatChange,
            StatDefData,
            StatModify,
            Summon,
            SurvivorLookup,
            TeleportRequest,
            TerrainEdit,
            TerrainMaterialInfo,
            TickData,
            UiDialog,
            UiFrame,
            UiMarker,
            UiWidget,
            WeatherFront,
            ZoneCheck
        )
    };
}

macro_rules! names {
    ($($ty:ident),*) => {
        [$(stringify!($ty)),*]
    };
}

macro_rules! each {
    ($($ty:ident),*) => {
        $(round_trip::<api_frame::$ty>(stringify!($ty), !NO_ZERO.contains(&stringify!($ty)));)*
    };
}

/// The headed structs with a function pointer that may not be null, which a
/// copy never fills with zero: the loader requires all of them.
const NO_ZERO: [&str; 2] = ["HostApi", "MigrationSet"];

/// The source of the ABI module.
const ABI: &str = include_str!("../../../../enshwick/src/abi.rs");
/// The source of the loader's entry points.
const HOST: &str = include_str!("../host.rs");

/// The structs of `abi.rs` whose first field is a [`Header`].
fn headed_in_abi() -> Vec<&'static str> {
    let mut found: Vec<&str> = ABI
        .split("\npub struct ")
        .skip(1)
        .filter_map(|rest| {
            let (name, body) = rest.split_once(" {")?;
            let first = body
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with("///"))?;
            (first == "pub header: Header,").then_some(name)
        })
        .collect();
    found.sort_unstable();
    found
}

/// A buffer of `bytes` bytes, 8-aligned, filled with [`GUARD`].
fn guarded(bytes: usize) -> Vec<u64> {
    vec![u64::from_ne_bytes([GUARD; 8]); bytes.div_ceil(8)]
}

fn bytes_of(buffer: &[u64]) -> &[u8] {
    // SAFETY: a `u64` slice is readable as bytes.
    unsafe { core::slice::from_raw_parts(buffer.as_ptr().cast(), size_of_val(buffer)) }
}

fn set_size(buffer: &mut [u64], at: usize, size: usize) {
    let size = u32::try_from(size).unwrap();
    // SAFETY: a `u64` slice is writable as bytes.
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), size_of_val(buffer))
    };
    bytes[at..at + 4].copy_from_slice(&size.to_ne_bytes());
}

fn size_at(buffer: &[u64], at: usize) -> usize {
    u32::from_ne_bytes(bytes_of(buffer)[at..at + 4].try_into().unwrap()) as usize
}

/// A value of `T` whose bytes are all [`VALUE`] past its header.
fn value<T: Copy>() -> T {
    let mut raw = vec![VALUE; size_of::<T>()];
    raw[..8].copy_from_slice(&[0; 8]);
    // SAFETY: the ABI structs hold integers, floats, raw pointers and
    // function pointers, for which a non-zero pattern is a valid value.
    let mut value = unsafe { raw.as_ptr().cast::<T>().read_unaligned() };
    // SAFETY: `T` starts with a header.
    unsafe {
        (&raw mut value)
            .cast::<Header>()
            .write_unaligned(Header::of::<T>());
    }
    value
}

/// Exchanges `T` with a mod that knows only its header and half of the rest;
/// `zero` when an all-zero `T` is valid, so a short copy may be read.
fn round_trip<T: Copy>(name: &str, zero: bool) {
    let full = size_of::<T>();
    let room = size_of::<Header>() + (full - size_of::<Header>()) / 2;

    let mut out = guarded(full + 16);
    set_size(&mut out, 0, room);
    assert_eq!(
        put_headed(out.as_mut_ptr().cast::<T>(), value::<T>()),
        Status::OK,
        "{name}"
    );
    let written = bytes_of(&out);
    assert_eq!(size_at(&out, 0), room, "{name}: the written size");
    assert!(
        written[8..room].iter().all(|&b| b == VALUE),
        "{name}: the known bytes"
    );
    assert!(
        written[room..].iter().all(|&b| b == GUARD),
        "{name}: wrote past the mod's struct"
    );

    let mut from = guarded(full + 16);
    set_size(&mut from, 0, room);
    if !zero {
        set_size(&mut from, 0, full);
        // SAFETY: a whole `T` in a larger buffer, a non-zero pattern each.
        let whole = unsafe { get_headed(from.as_ptr().cast::<T>(), full) };
        assert!(whole.is_some(), "{name}");
        set_size(&mut from, 0, room);
        // SAFETY: as above, shorter.
        let short = unsafe { get_headed(from.as_ptr().cast::<T>(), full) };
        assert!(short.is_none(), "{name}");
        return;
    }
    // SAFETY: a header of `room` bytes in a larger buffer.
    let copy = unsafe { get_headed(from.as_ptr().cast::<T>(), room) }.expect(name);
    // SAFETY: a `T` is readable as bytes.
    let copied = unsafe { core::slice::from_raw_parts((&raw const copy).cast::<u8>(), full) };
    assert!(
        copied[8..room].iter().all(|&b| b == GUARD),
        "{name}: the known bytes"
    );
    assert!(
        copied[room..].iter().all(|&b| b == 0),
        "{name}: read past the mod's struct"
    );
    if room < full {
        // SAFETY: as above.
        let short = unsafe { get_headed(from.as_ptr().cast::<T>(), full) };
        assert!(
            short.is_none(),
            "{name}: a struct short of the required fields"
        );
    }

    let capacity = 3;
    let mut list = guarded(room * capacity + 16);
    set_size(&mut list, 0, room);
    let items = [value::<T>(); 4];
    let mut count = 0;
    let status = put_headed_list(&items, list.as_mut_ptr().cast::<T>(), 3, &raw mut count);
    assert_eq!((status, count), (Status::OK, 4), "{name}");
    for entry in 0..capacity {
        assert_eq!(size_at(&list, entry * room), room, "{name}: entry {entry}");
    }
    assert!(
        bytes_of(&list)[room * capacity..]
            .iter()
            .all(|&b| b == GUARD),
        "{name}: a list written past its capacity"
    );

    // SAFETY: `capacity` entries of `room` bytes, each with its header.
    let read = unsafe { get_headed_list(list.as_ptr().cast::<T>(), 3, room) }.expect(name);
    assert_eq!(read.len(), capacity, "{name}");
    set_size(&mut list, room, room + 8);
    // SAFETY: as above.
    let mixed = unsafe { get_headed_list(list.as_ptr().cast::<T>(), 3, room) };
    assert!(mixed.is_none(), "{name}: entries of different sizes");
}

#[test]
fn the_list_of_headed_structs_is_the_one_in_the_abi() {
    let mut listed = headed!(names).to_vec();
    listed.sort_unstable();
    assert_eq!(listed, headed_in_abi());
}

#[test]
fn every_headed_struct_crosses_only_as_far_as_the_older_side_knows_it() {
    headed!(each);
}

#[test]
fn a_list_whose_first_header_is_shorter_than_a_header_is_refused() {
    let mut list = guarded(64);
    set_size(&mut list, 0, 4);
    let mut count = 0;
    let out = list.as_mut_ptr().cast::<api_frame::WeatherFront>();
    let status = put_headed_list(
        &[value::<api_frame::WeatherFront>()],
        out,
        1,
        &raw mut count,
    );
    assert_eq!(status, Status::BAD_ARGS);
    assert!(bytes_of(&list)[4..].iter().all(|&b| b == GUARD));
}

/// Every entry point that takes a pointer to a headed struct reads or writes
/// it through a helper that knows the header.
#[test]
fn every_entry_point_passes_headed_pointers_through_a_sized_helper() {
    const HELPERS: [&str; 6] = [
        "get_headed(",
        "put_headed(",
        "get_headed_list(",
        "put_headed_list(",
        "read_headed(",
        "list(",
    ];
    let headed = headed_in_abi();
    let mut checked = 0;
    for chunk in HOST.split("extern \"system\" fn ").skip(1) {
        let Some((signature, body)) = chunk.split_once('{') else {
            continue;
        };
        let name = signature.split('(').next().unwrap_or_default();
        for param in signature.split(',') {
            let Some((param, ty)) = param.split_once(": ") else {
                continue;
            };
            let param = param.rsplit(['(', ' ', '\n']).next().unwrap_or_default();
            let ty = ty.split(')').next().unwrap_or_default().trim();
            let Some(pointee) = ty
                .strip_prefix("*const ")
                .or_else(|| ty.strip_prefix("*mut "))
            else {
                continue;
            };
            let pointee = pointee.rsplit("::").next().unwrap_or_default();
            if !headed.contains(&pointee) {
                continue;
            }
            let body = body.split("\nextern \"system\" fn ").next().unwrap_or(body);
            let routed = HELPERS.iter().any(|helper| {
                body.match_indices(helper).any(|(at, _)| {
                    let args = &body[at + helper.len()..];
                    let args = &args[..args.find(')').unwrap_or(args.len())];
                    args.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|w| w == param)
                })
            });
            assert!(
                routed,
                "{name}: `{param}: {ty}` is not read or written by its header"
            );
            for raw in [".as_ref()", ".as_mut()", ".write(", ".add("] {
                assert!(
                    !body.contains(&format!("{param}{raw}")),
                    "{name}: `{param}{raw}` ignores the mod's header"
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 30, "only {checked} headed pointers found");
}
