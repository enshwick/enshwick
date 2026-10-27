//! Mod events described at run time, such as by the schema a script
//! carries: the same subscription and definition as for an
//! [`#[event]`](macro@crate::event) type, with the data as bytes in the
//! defining mod's layout.

use core::ffi::c_void;

use crate::abi::{Header, Layout, LayoutField, Str};
use crate::load::{hand_over, ok};
use crate::points::DescribedField;
use crate::{Host, Load, Off};

/// A mod event as its schema describes it, for code that reads and writes
/// its data as bytes.
///
/// # Examples
///
/// ```
/// use enshwick::points::DescribedField;
/// use enshwick::services::DescribedEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let sold = DescribedEvent::new("you.trading-post:sold", vec![DescribedField::new("item", 1)], 4);
///     load.on_described_mod_event(&sold, |data| enshwick::debug!("{} bytes", data.len()))
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DescribedEvent {
    /// The event's name, `<defining mod id>:<what>`.
    pub name: String,
    /// Its fields, in the defining mod's order, each with its layout hash.
    pub fields: Vec<DescribedField>,
    /// The size of those fields, the bytes a handler reads.
    pub size: u32,
}

impl DescribedEvent {
    /// The event `name` with `fields`, `size` bytes.
    ///
    /// # Examples
    ///
    /// See [`DescribedEvent`].
    #[must_use]
    pub fn new(name: &str, fields: Vec<DescribedField>, size: u32) -> Self {
        Self {
            name: name.to_owned(),
            fields,
            size,
        }
    }

    /// Calls `with` with the event's ABI spec, which borrows from `self`.
    fn with_spec<R>(&self, with: impl FnOnce(&crate::abi::EventSpec) -> R) -> R {
        let fields: Vec<LayoutField> = self
            .fields
            .iter()
            .map(|f| LayoutField {
                name: Str::new(&f.name),
                hash: f.hash,
            })
            .collect();
        let spec = crate::abi::EventSpec {
            header: Header::of::<crate::abi::EventSpec>(),
            name: Str::new(&self.name),
            layout: Layout {
                fields: fields.as_ptr(),
                count: u32::try_from(fields.len()).unwrap_or(u32::MAX),
                size: self.size,
            },
        };
        with(&spec)
    }
}

/// Sends a described mod event this mod defined, from any thread.
///
/// # Examples
///
/// ```
/// use enshwick::points::DescribedField;
/// use enshwick::services::DescribedEvent;
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let greeted = DescribedEvent::new("you.greeter:greeted", vec![DescribedField::new("player", 1)], 8);
///     // SAFETY: the one field is a `u64`, for which any bytes are valid.
///     let send = unsafe { load.define_described_event(&greeted) }?;
///     send.send(&7u64.to_le_bytes());
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct DescribedEmitter {
    host: Host,
    id: u32,
    size: usize,
}

impl DescribedEmitter {
    /// Hands `data`, the event's bytes, to every subscriber on this thread;
    /// data of another size than the event's is not sent.
    ///
    /// # Examples
    ///
    /// See [`DescribedEmitter`].
    pub fn send(&self, data: &[u8]) {
        if data.len() != self.size {
            return;
        }
        let _status =
            (self.host.api().send_event)(self.host.handle(), self.id, data.as_ptr().cast());
    }
}

/// A described handler and how many bytes it reads.
struct Handler<F> {
    size: usize,
    run: F,
}

impl Load<'_> {
    /// Calls `handler` with the data of every described mod event `event`
    /// another mod sends, as [`on_mod_event`](Self::on_mod_event) does for a
    /// type: the loader checks the layout hashes against the definition.
    ///
    /// # Errors
    ///
    /// [`Off`] when called after `init`.
    ///
    /// # Examples
    ///
    /// See [`DescribedEvent`].
    pub fn on_described_mod_event<F>(
        &mut self,
        event: &DescribedEvent,
        handler: F,
    ) -> Result<(), Off>
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        let host = self.host();
        let handler = Handler {
            size: usize::try_from(event.size).unwrap_or(0),
            run: handler,
        };
        event.with_spec(|spec| {
            hand_over(
                handler,
                |user| {
                    (host.api().subscribe_event)(host.handle(), spec, deliver_described::<F>, user)
                },
                &format!("event {}", event.name),
            )
        })
    }

    /// Defines the described mod event `event` for other mods to subscribe
    /// to, as [`define_event`](Self::define_event) does for a type.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is outside this mod's id or defined twice.
    ///
    /// # Safety
    ///
    /// - Every byte pattern sent through the returned emitter is a valid
    ///   value of the type each field's hash names, since a subscriber reads
    ///   the data as that type: a `bool` is 0 or 1, an enum one of its
    ///   variants.
    ///
    /// # Examples
    ///
    /// See [`DescribedEmitter`].
    pub unsafe fn define_described_event(
        &mut self,
        event: &DescribedEvent,
    ) -> Result<DescribedEmitter, Off> {
        let host = self.host();
        let mut id = 0;
        let status =
            event.with_spec(|spec| (host.api().define_event)(host.handle(), spec, &raw mut id));
        ok(
            status,
            DescribedEmitter {
                host,
                id,
                size: usize::try_from(event.size).unwrap_or(0),
            },
            || {
                format!(
                    "event {}: named after another mod or defined twice",
                    event.name
                )
            },
        )
    }
}

/// Runs a described handler; `user` is the [`Handler`] boxed by
/// [`Load::on_described_mod_event`], alive for the process.
unsafe extern "system" fn deliver_described<F: Fn(&[u8])>(data: *const c_void, user: *mut c_void) {
    // SAFETY: `user` is the leaked `Box<Handler<F>>` this function was
    // subscribed with.
    let handler = unsafe { &*user.cast::<Handler<F>>() };
    if data.is_null() {
        return;
    }
    // SAFETY: the loader checked the sender's layout starts with every
    // described field, `size` bytes, and the data lives for the call.
    let data = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), handler.size) };
    crate::host::guard(
        "panic in a mod event handler",
        || (),
        || (handler.run)(data),
    );
}
