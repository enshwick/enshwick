//! Build table ids under `join.`.

use super::Constant;

pub(crate) const TARGET_COPY: Constant = Constant::new("join.target_copy");
pub(crate) const TARGET_GAME_PORT_READ: Constant = Constant::new("join.target_game_port_read");
pub(crate) const TARGET_HAS_ADDRESS: Constant = Constant::new("join.target_has_address");
pub(crate) const TARGET_HOST: Constant = Constant::new("join.target_host");
pub(crate) const TARGET_IP_READ: Constant = Constant::new("join.target_ip_read");
pub(crate) const TARGET_PASSWORD: Constant = Constant::new("join.target_password");
pub(crate) const TARGET_QUERY_PORT_READ: Constant = Constant::new("join.target_query_port_read");
pub(crate) const TARGET_WORD_COPY: Constant = Constant::new("join.target_word_copy");
