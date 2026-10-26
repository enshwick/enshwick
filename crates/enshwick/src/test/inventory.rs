//! Inventory requests the mod submits and their outcomes.

use super::FakeHost;
use super::Installed;
use super::table;
use crate::abi::Header;
use crate::abi::InventoryOp;
use crate::abi::InventoryOpResult;
use crate::abi::InventoryResult;

impl FakeHost {
    /// Plays a game build without inventory transactions: `inventory_open`
    /// answers [`Status::MISSING`](crate::abi::Status::MISSING).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().without_inventories().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_inventories(mut self) -> Self {
        self.no_inventories = true;
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The inventory requests the mod submitted that are not finished yet, in
    /// order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.inventory_requests().is_empty());
    /// ```
    #[must_use]
    pub fn inventory_requests(&self) -> Vec<SubmittedRequest> {
        table::with(Vec::new(), |s| {
            s.inventory_requests
                .iter()
                .map(|r| r.request.clone())
                .collect()
        })
    }

    /// Finishes the oldest open inventory request as the game would:
    /// `outcome` is an `InventoryResult` constant and `ops` a result per
    /// operation. The mod's callback runs here, as on the game thread.
    /// Returns false when no request is open.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::InventoryResult;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(!loader.finish_inventory(InventoryResult::COMMITTED, &[]));
    /// ```
    #[must_use = "false means no request was open"]
    pub fn finish_inventory(&self, outcome: u32, ops: &[InventoryOpResult]) -> bool {
        let open = table::with(None, |s| {
            (!s.inventory_requests.is_empty()).then(|| s.inventory_requests.remove(0))
        });
        let Some(open) = open else {
            return false;
        };
        let result = InventoryResult {
            header: Header::of::<InventoryResult>(),
            outcome,
            op_count: u32::try_from(ops.len()).unwrap_or(0),
            ops: ops.as_ptr(),
        };
        // SAFETY: the mod's callback with its pointer, handed back once; the
        // result lives for the call.
        unsafe {
            (open.done)(
                &raw const result,
                core::ptr::with_exposed_provenance_mut(open.user),
            );
        }
        true
    }
}

/// An inventory request the mod submitted, as the loader received it.
///
/// # Examples
///
/// See [`Installed::inventory_requests`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SubmittedRequest {
    /// The operations, in order.
    pub ops: Vec<InventoryOp>,
    /// `InventoryRequest` flags.
    pub flags: u32,
    /// The player credited, 0 for the game's choice.
    pub actor: u32,
}
