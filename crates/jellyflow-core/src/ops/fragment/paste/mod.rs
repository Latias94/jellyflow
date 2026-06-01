mod planner;
mod remapped_ids;
mod tuning;

pub use tuning::PasteTuning;

use crate::ops::GraphTransaction;

use super::model::GraphFragment;
use super::remap::IdRemapper;

impl GraphFragment {
    /// Remaps IDs and produces a transaction that inserts the fragment into a graph.
    ///
    /// The resulting transaction is deterministic for a given seed.
    pub fn to_paste_transaction(
        &self,
        remapper: &IdRemapper,
        tuning: PasteTuning,
    ) -> GraphTransaction {
        planner::FragmentPastePlanner::new(self, remapper, tuning).finish()
    }
}
