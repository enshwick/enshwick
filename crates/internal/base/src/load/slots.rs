//! The state a domain keeps on [`Load`](crate::Load) while a mod initialises,
//! one slot per type, so `Load` names no domain.

use core::any::Any;
use core::panic::{RefUnwindSafe, UnwindSafe};

/// What a slot may hold: what keeps `Load` `Send`, `Sync` and unwind safe.
pub trait Slot: Any + Send + Sync + UnwindSafe + RefUnwindSafe {}

impl<T: Any + Send + Sync + UnwindSafe + RefUnwindSafe> Slot for T {}

/// One `Option<T>` per type `T` a domain asks for, empty until it fills it.
#[derive(Default)]
pub(crate) struct Slots(Vec<Box<dyn Slot>>);

impl Slots {
    /// The slot of `T`, made empty on first use. A domain keys it by a type
    /// of its own, so no other domain reaches it.
    pub(crate) fn of<T: Slot>(&mut self) -> &mut Option<T> {
        let at = self
            .0
            .iter()
            .position(|slot| (&**slot as &dyn Any).is::<Option<T>>())
            .unwrap_or_else(|| {
                self.0.push(Box::new(None::<T>));
                self.0.len() - 1
            });
        match (&mut *self.0[at] as &mut dyn Any).downcast_mut() {
            Some(slot) => slot,
            None => unreachable!("slot {at} holds an Option of its type"),
        }
    }
}

impl core::fmt::Debug for Slots {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Slots({})", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::Slots;

    #[test]
    fn slots_keep_load_send_sync_and_unwind_safe() {
        fn held<T: Send + Sync + core::panic::UnwindSafe + core::panic::RefUnwindSafe>() {}
        held::<Slots>();
        held::<crate::Load<'static>>();
    }

    #[test]
    fn each_type_keeps_its_own_slot_until_it_is_filled() {
        let mut slots = Slots::default();
        assert_eq!(*slots.of::<u32>(), None);
        *slots.of::<u32>() = Some(7);
        *slots.of::<u64>() = Some(9);
        *slots.of::<u32>().get_or_insert(0) += 1;
        assert_eq!(*slots.of::<u32>(), Some(8));
        assert_eq!(*slots.of::<u64>(), Some(9));
        assert_eq!(format!("{slots:?}"), "Slots(2)");
    }
}
