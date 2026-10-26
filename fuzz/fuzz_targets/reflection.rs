//! A game image: the PE headers and the engine's reflection data, read from
//! arbitrary bytes as a file and as a loaded module, also at the top of the
//! address space.

#![no_main]

use enshwick_gamedata::reflect::{Image, Layout, Reflection};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let top = Some(u64::MAX - 0x100);
    for (layout, base) in [
        (Layout::File, None),
        (Layout::Mapped, None),
        (Layout::File, top),
    ] {
        let Ok(image) = Image::parse(bytes, layout, base) else {
            continue;
        };
        let _ = image.fingerprint();
        // The scan walks every address of `.rdata`'s range, up to 4 GiB a
        // short input never backs: correct, but too slow to fuzz.
        if image.section(".rdata").is_some_and(|rdata| {
            let range = image.range(rdata);
            range.end - range.start > bytes.len() as u64
        }) {
            continue;
        }
        let reflection = Reflection::scan(&image);
        for name in reflection.type_names().take(64) {
            if let Ok(info) = reflection.type_info(name) {
                for field in info.fields() {
                    drop(reflection.field_offset(name, field.name()));
                }
            }
        }
    }
});
