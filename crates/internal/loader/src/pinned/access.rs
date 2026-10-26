//! Build table ids under `access.`.

use super::{Constant, Symbol};

pub(crate) const BASE_STOCK: Symbol = Symbol::new("access.base_stock");
pub(crate) const BASE_STOCK_UI_RETURN: Symbol = Symbol::new("access.base_stock_ui_return");
pub(crate) const CONSUMED_DEPOSIT: Constant = Constant::new("access.consumed_deposit");
pub(crate) const CONSUMED_UPGRADE: Constant = Constant::new("access.consumed_upgrade");
pub(crate) const DEPOSIT_TARGET: Constant = Constant::new("access.deposit_target");
pub(crate) const DEPOSIT_VERSION: Constant = Constant::new("access.deposit_version");
pub(crate) const DISMANTLE_METHOD: Symbol = Symbol::new("access.dismantle_method");
pub(crate) const OBJECT_ID: Constant = Constant::new("access.object_id");
pub(crate) const OBJECT_KIND: Constant = Constant::new("access.object_kind");
pub(crate) const OFFER_TEST: Symbol = Symbol::new("access.offer_test");
pub(crate) const SORT: Symbol = Symbol::new("access.sort");
pub(crate) const SORT_TARGET: Constant = Constant::new("access.sort_target");
pub(crate) const STATE_INPUT: Constant = Constant::new("access.state_input");
pub(crate) const STATE_OWNER_INDEX: Constant = Constant::new("access.state_owner_index");
pub(crate) const TRANSFER: Symbol = Symbol::new("access.transfer");
pub(crate) const TRANSFER_SOURCE: Constant = Constant::new("access.transfer_source");
pub(crate) const TRANSFER_SOURCE_SLOT: Constant = Constant::new("access.transfer_source_slot");
pub(crate) const TRANSFER_TARGET: Constant = Constant::new("access.transfer_target");
pub(crate) const TRANSFER_TARGET_SLOT: Constant = Constant::new("access.transfer_target_slot");
pub(crate) const UPGRADE_TARGET: Constant = Constant::new("access.upgrade_target");
pub(crate) const UPGRADE_VERSION: Constant = Constant::new("access.upgrade_version");
