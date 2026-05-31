use serde::{Deserialize, Serialize};

use crate::core::PortId;

/// Edge endpoint pair (from/to ports).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeEndpoints {
    pub from: PortId,
    pub to: PortId,
}
