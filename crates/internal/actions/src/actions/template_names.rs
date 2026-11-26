//! The vanilla templates' names, read on a thread of their own as each world
//! loads once a mod's action takes a template, so the first console line that
//! names one finds them read: on the game thread the read took about 139 ms
//! on a dedicated server (26083076).

use core::ffi::c_void;
use core::sync::atomic::Ordering;

use super::Arg;
use super::registry::Registry;
use super::sheet::ArgView;

/// Has the names read aside as each world loads, from the first action of
/// `registry` whose `args` take a template; one added after init, when no
/// event takes a handler, leaves them to the first console line that names
/// a template.
pub(super) fn watch(registry: &Registry, args: &[Arg]) {
    if !args.iter().any(|arg| arg.kind == ArgView::TEMPLATE)
        || registry.templates_watched.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let host = registry.host;
    let _late = crate::load::hand_over(
        (),
        |user| {
            (host.api().subscribe)(
                host.handle(),
                crate::abi::Event::WORLD_LOADED,
                Some(world_loaded),
                user,
            )
        },
        "the template names' read",
    );
}

/// Reads the names on a thread of their own.
///
/// # Safety
///
/// None: it reads no argument.
unsafe extern "system" fn world_loaded(
    _event: crate::abi::Event,
    _data: *const c_void,
    _user: *mut c_void,
) {
    let _caught = crate::host::caught(|| {
        let Some(vanilla) = crate::content::vanilla::Vanilla::game() else {
            return;
        };
        if let Err(error) = crate::host::__spawn("template-names", move || {
            enshwick_content::__internal::read_template_names(vanilla);
        }) {
            crate::warn!(
                "the template names are read at the first console line naming one: {error}"
            );
        }
    });
}
