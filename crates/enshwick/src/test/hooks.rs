//! Game lookups and hooks: symbols, constants, layouts, detours and code patches.

use crate::FnPtr;
use crate::Hook;
use crate::abi::CallerFrame;
use crate::abi::Fingerprint;
use crate::abi::HookFlags;

use super::FakeHost;
use super::Installed;
use super::address;
use super::table;

impl FakeHost {
    /// The game's image base.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().base(0x1_4000_0000).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn base(mut self, base: u64) -> Self {
        self.base = base;
        self
    }

    /// The game build's fingerprint.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Fingerprint, guid};
    ///
    /// let build = Fingerprint { guid: guid("1c129660cd6efc4f9f0b21652d5ff2bf"), age: 387 };
    /// let loader = enshwick::test::FakeHost::new().fingerprint(build).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn fingerprint(mut self, fingerprint: Fingerprint) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// Where the build table comes from; [`BuildTable::EXACT`](crate::abi::BuildTable::EXACT)
    /// by default.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::BuildTable;
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .build_table(BuildTable::PATTERNS)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn build_table(mut self, table: crate::abi::BuildTable) -> Self {
        self.table = table;
        self
    }

    /// The address `Load::symbol(name)` finds.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn symbol(mut self, name: &str, address: u64) -> Self {
        self.symbols.insert(name.to_owned(), address);
        self
    }

    /// The address `Load::system(name)` finds.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn system(mut self, name: &str, address: u64) -> Self {
        self.systems.insert(name.to_owned(), address);
        self
    }

    /// The value `Load::constant(name)` finds, such as the offset an Enshwick
    /// id names.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn constant(mut self, name: &str, value: u64) -> Self {
        self.constants.insert(name.to_owned(), value);
        self
    }

    /// The offset `Load::field_offset(type_name, field)` finds.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .field_offset("keen::ecs::TriggerShape", "rangeX", 4)
    ///     .type_size("keen::ecs::TriggerShape", 16)
    ///     .type_hash("keen::ecs::Name", 0x027a_c564)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn field_offset(mut self, type_name: &str, field: &str, offset: u64) -> Self {
        self.field_offsets
            .insert((type_name.to_owned(), field.to_owned()), offset);
        self
    }

    /// The size `Load::type_size(type_name)` finds, by reflected name or
    /// Enshwick id.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::field_offset`].
    #[must_use]
    pub fn type_size(mut self, type_name: &str, size: u32) -> Self {
        self.type_sizes.insert(type_name.to_owned(), size);
        self
    }

    /// The ECS type hash of the component or resource type `type_name`, by
    /// reflected name or Enshwick id.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::field_offset`].
    #[must_use]
    pub fn type_hash(mut self, type_name: &str, hash: u32) -> Self {
        self.type_hashes.insert(type_name.to_owned(), hash);
        self
    }

    /// The caller's context a hook with
    /// [`HookFlags::CALLER_FRAME`] reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::CallerFrame;
    ///
    /// let frame = CallerFrame { rsp: 0x1000, ..CallerFrame::default() };
    /// let loader = enshwick::test::FakeHost::new().caller_frame(frame).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn caller_frame(mut self, frame: CallerFrame) -> Self {
        self.caller_frame = frame;
        self
    }

    /// Refuses hooks on `target`, as when another mod holds it exclusively.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().refuse_hook(0x1_4009_0a60).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn refuse_hook(mut self, target: u64) -> Self {
        self.refused_hooks.insert(target);
        self
    }

    /// Answers `code` as the game's code section, for pattern searches.
    ///
    /// # Examples
    ///
    /// ```
    /// static CODE: [u8; 4] = [0x90, 0xb9, 0x00, 0x02];
    /// let loader = enshwick::test::FakeHost::new().code(&CODE).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn code(mut self, code: &'static [u8]) -> Self {
        self.code = (code.as_ptr().addr() as u64, code.len() as u64);
        self
    }

    /// The most recipes one workshop takes; 512 unless set, [`u32::MAX`]
    /// for a lifted limit.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().workshop_recipes(u32::MAX).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn workshop_recipes(mut self, limit: u32) -> Self {
        self.workshop_recipes = limit;
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The hooks the mod linked, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.hooks().iter().all(|hook| hook.priority == 0));
    /// ```
    #[must_use]
    pub fn hooks(&self) -> Vec<LinkedHook> {
        table::with(Vec::new(), |s| s.hooks.clone())
    }

    /// The addresses the mod placed mid-function hooks at, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.mid_hooks().is_empty());
    /// ```
    #[must_use]
    pub fn mid_hooks(&self) -> Vec<u64> {
        table::with(Vec::new(), |s| s.mid_hooks.iter().map(|h| h.0).collect())
    }

    /// Runs the mod's mid-function hooks at `address` on `registers`, as the
    /// game reaching that instruction would; returns how many ran.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let mut registers = enshwick::abi::Registers::default();
    /// assert_eq!(loader.run_at(0x1_4000_1000, &mut registers), 0);
    /// ```
    #[must_use = "the count says whether the mod hooks the address"]
    pub fn run_at(&self, address: u64, registers: &mut crate::abi::Registers) -> usize {
        let handlers: Vec<_> = table::with(Vec::new(), |s| {
            s.mid_hooks
                .iter()
                .filter(|h| h.0 == address)
                .map(|h| (h.1, h.2))
                .collect()
        });
        for &(handler, user) in &handlers {
            // SAFETY: the registers are valid for the call, and `user` is the
            // pointer the mod registered the handler with.
            unsafe { handler(registers, core::ptr::with_exposed_provenance_mut(user)) };
        }
        handlers.len()
    }

    /// The entry point of `hook` when the mod linked it: call it to run the
    /// detour as the game would, its chain continuing to the target.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn detour<F: FnPtr>(&self, hook: &'static Hook<F>) -> Option<F> {
        let entry = address(hook.entry());
        self.hooks()
            .iter()
            .any(|linked| linked.detour == entry)
            .then(|| hook.entry())
    }

    /// The code patches the mod asked for, as `(address, expected,
    /// replacement)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.code_patches().is_empty());
    /// ```
    #[must_use]
    pub fn code_patches(&self) -> Vec<(u64, Vec<u8>, Vec<u8>)> {
        table::with(Vec::new(), |s| s.code_patches.clone())
    }
}

/// A hook the mod linked, as the loader received it.
///
/// # Examples
///
/// See [`Installed::hooks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LinkedHook {
    /// The address of the hooked function.
    pub target: u64,
    /// The address of the detour's entry point.
    pub detour: u64,
    /// Lower runs earlier.
    pub priority: i32,
    /// The hook's flags.
    pub flags: HookFlags,
}
