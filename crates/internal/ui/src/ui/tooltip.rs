//! Lines a mod adds to the game's item tooltips, per item.

use core::ffi::c_void;
use core::marker::PhantomData;
use std::sync::Mutex;

use crate::abi::{Str, UiTooltipItem, UiWidget};
use crate::inventory::ItemInstance;
use crate::locks::Relock;
use crate::{Host, Off};

use super::Label;
use super::client_only;

/// The lines a mod adds to one item's tooltip, lent to the handler of
/// [`Load::extend_item_tooltip`](../struct.Load.html#method.extend_item_tooltip)
/// for its call.
///
/// # Examples
///
/// See [`Load::extend_item_tooltip`](../struct.Load.html#method.extend_item_tooltip).
#[derive(Debug)]
pub struct TooltipLines<'t> {
    pub(super) host: Host,
    pub(super) token: u64,
    pub(super) _life: PhantomData<&'t mut ()>,
    pub(super) _thread: PhantomData<*const ()>,
}

impl TooltipLines<'_> {
    /// Adds a line under the game's rows, in the label's style; false when
    /// the loader refused it, as past
    /// [`UiTooltipItem::MAX_LINES`](crate::abi::UiTooltipItem::MAX_LINES)
    /// lines of the handler.
    ///
    /// # Examples
    ///
    /// See [`Load::extend_item_tooltip`](../struct.Load.html#method.extend_item_tooltip).
    pub fn push<'a>(&mut self, label: impl Into<Label<'a>>) -> bool {
        let label = label.into();
        let mut widget = enshwick_abi::headed!(UiWidget {
            kind: UiWidget::LABEL,
            flags: label.flags,
            style: label.style.to_raw(),
            text: Str::new(label.text),
            ..UiWidget::EMPTY
        });
        (self.host.api().ui_widget)(self.host.handle(), self.token, &raw mut widget).is_ok()
    }
}

/// A tooltip handler as the loader keeps it, with the host of the mod that
/// registered it.
struct Handler<F> {
    host: Host,
    handler: Mutex<F>,
}

unsafe extern "system" fn tooltip<F>(item: *const UiTooltipItem, user: *mut c_void)
where
    F: FnMut(ItemInstance, &mut TooltipLines<'_>),
{
    // SAFETY: the loader passes an item readable for the size its header
    // states, for the call.
    let Some(raw) = (unsafe { crate::abi::read_headed(item, size_of::<UiTooltipItem>()) }) else {
        return;
    };
    // SAFETY: `user` is the leaked `Handler<F>` this function was registered
    // with.
    let handler = unsafe { &*user.cast::<Handler<F>>() };
    let host = handler.host;
    crate::host::guard(
        "panic in an item tooltip handler",
        || (),
        || {
            let mut run = handler.handler.relock();
            (run)(
                ItemInstance {
                    pide: raw.pide,
                    item: raw.item,
                },
                &mut TooltipLines {
                    host,
                    token: raw.token,
                    _life: PhantomData,
                    _thread: PhantomData,
                },
            );
        },
    );
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::extend_item_tooltip`.
    #[doc(hidden)]
    fn extend_item_tooltip<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: FnMut(ItemInstance, &mut TooltipLines<'_>) + Send + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn extend_item_tooltip<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: FnMut(ItemInstance, &mut TooltipLines<'_>) + Send + 'static,
    {
        let host = self.host();
        client_only(host, "item tooltips")?;
        crate::load::hand_over(
            Handler {
                host,
                handler: Mutex::new(handler),
            },
            |user| (host.api().ui_item_tooltip)(host.handle(), Some(tooltip::<F>), user),
            "item tooltip",
        )
    }
}
