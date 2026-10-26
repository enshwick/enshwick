//! Every struct that starts with a [`Header`] crosses the boundary only as far
//! as both sides know it, so a mod built against an older minor stays safe.

use enshwick::abi::Headed;

use super::memory::{get_headed, get_headed_list, put_headed, put_headed_list};
use super::*;

/// The fill of a value's own bytes.
const VALUE: u8 = 0xAB;
/// The fill of the bytes a mod owns past its struct.
const GUARD: u8 = 0x11;

/// Every struct of the ABI that implements [`Headed`]; the tests below hold
/// this list to `abi.rs`.
macro_rules! headed {
    ($check:ident) => {
        $check!(
            ActionCall,
            ActiveBuff,
            AffixDefData,
            AffixRoll,
            AltarInfo,
            AltarZone,
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
            DataChange,
            DaytimeInfo,
            EventSpec,
            EventTypeInfo,
            ExperiencePass,
            GameEventData,
            Interaction,
            InventoryRequest,
            InventoryResult,
            ItemData,
            KnowledgeData,
            KnowledgeInfo,
            KnowledgeInput,
            LoopWaitData,
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
        $(round_trip::<api_frame::$ty>(stringify!($ty));)*
    };
}

/// The one struct that starts with a [`Header`] without being [`Headed`]:
/// the loader hands out its own static table and never copies a mod's.
const NOT_HEADED: &str = "HostApi";

/// The source of the module `<dir>.rs` and of every file below `<dir>/`, read
/// once for the process.
fn module_source(dir: &str) -> String {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut files = vec![dir.with_extension("rs")];
    walk(&dir, &mut files);
    files.sort();
    files
        .iter()
        .map(|path| {
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The source of the ABI module and its submodules.
fn abi_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| module_source("../../enshwick/src/abi"))
}

/// The source of the loader's entry points.
fn host_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| module_source("src/host"))
}

/// The structs of `abi.rs` that implement [`Headed`].
fn headed_in_abi() -> Vec<&'static str> {
    let mut found: Vec<&str> = abi_source()
        .split("\nimpl_headed!(")
        .skip(1)
        .filter_map(|rest| rest.split_once(')').map(|(name, _)| name))
        .collect();
    found.sort_unstable();
    found
}

/// The structs of `abi.rs` whose first field is a [`Header`].
fn header_first_in_abi() -> Vec<&'static str> {
    let mut found: Vec<&str> = abi_source()
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
    // SAFETY: the slice is live, initialised and readable for its size in
    // bytes, and `u8` has no alignment or validity requirement.
    unsafe { core::slice::from_raw_parts(buffer.as_ptr().cast(), size_of_val(buffer)) }
}

fn set_size(buffer: &mut [u64], at: usize, size: usize) {
    let size = u32::try_from(size).unwrap();
    // SAFETY: the slice is borrowed mutably, initialised and writable for its
    // size in bytes, and `u8` has no alignment or validity requirement.
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), size_of_val(buffer))
    };
    bytes[at..at + 4].copy_from_slice(&size.to_ne_bytes());
}

fn size_at(buffer: &[u64], at: usize) -> usize {
    u32::from_ne_bytes(bytes_of(buffer)[at..at + 4].try_into().unwrap()) as usize
}

/// A value of `T` whose bytes are all [`VALUE`] past its header.
fn value<T: Headed>() -> T {
    let mut raw = vec![VALUE; size_of::<T>()];
    raw[..size_of::<Header>()].copy_from_slice(bytemuck::bytes_of(&Header::of::<T>()));
    // SAFETY: `raw` holds `size_of::<T>()` initialised bytes; past the header
    // they are all 0xAB, a valid value for the integer, float, raw pointer
    // and optional function pointer fields of every `Headed` struct.
    unsafe { raw.as_ptr().cast::<T>().read_unaligned() }
}

/// Exchanges `T` with a mod that knows only its header and half of the rest.
fn round_trip<T: Headed>(name: &str) {
    let full = size_of::<T>();
    let room = size_of::<Header>() + (full - size_of::<Header>()) / 2;

    let mut out = guarded(full + 16);
    set_size(&mut out, 0, room);
    // SAFETY: `out` is a local buffer writable for `full + 16` bytes, and its
    // header states `room`, fewer; nothing else touches it.
    let status = unsafe { put_headed(out.as_mut_ptr().cast::<T>(), value::<T>()) };
    assert_eq!(status, Status::OK, "{name}");
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
    // SAFETY: `from` is readable for `full + 16` bytes and its header states
    // `room`, fewer.
    let copy = unsafe { get_headed(from.as_ptr().cast::<T>(), room) }.expect(name);
    // SAFETY: `copy` is a live local `T`, readable for its `full` bytes; the
    // read-back bytes are those `get_headed` copied or zeroed, all initialised.
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
        // SAFETY: `from` is readable for `full + 16` bytes and its header
        // states `room`, fewer.
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
    // SAFETY: `list` is a local buffer writable for `room * capacity + 16`
    // bytes, `capacity` entries of the `room` bytes its first header states,
    // and `count` a local `u32`; nothing else touches either.
    let status =
        unsafe { put_headed_list(&items, list.as_mut_ptr().cast::<T>(), 3, &raw mut count) };
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

    // SAFETY: `list` is readable for `room * capacity + 16` bytes: `capacity`
    // entries of `room` bytes, each with its header.
    let read = unsafe { get_headed_list(list.as_ptr().cast::<T>(), 3, room) }.expect(name);
    assert_eq!(read.len(), capacity, "{name}");
    set_size(&mut list, room, room + 8);
    // SAFETY: `list` is readable for `room * capacity + 16` bytes, which
    // covers every entry at the stride its first header states.
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
fn every_struct_that_starts_with_a_header_is_headed_but_the_host_table() {
    let mut expected = headed_in_abi();
    expected.push(NOT_HEADED);
    expected.sort_unstable();
    assert_eq!(header_first_in_abi(), expected);
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
    // SAFETY: `out` is a local buffer writable for 64 bytes and `count` a
    // local `u32`; nothing else touches either.
    let status = unsafe {
        put_headed_list(
            &[value::<api_frame::WeatherFront>()],
            out,
            1,
            &raw mut count,
        )
    };
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
    for chunk in host_source().split("extern \"system\" fn ").skip(1) {
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
