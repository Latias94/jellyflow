use std::collections::VecDeque;

#[cfg(test)]
use std::cell::Cell;

use crate::graph_kernel::{GraphTransaction, normalize_transaction};

/// Maximum number of transactions retained by default.
pub const DEFAULT_HISTORY_LIMIT: usize = 256;

#[cfg(test)]
thread_local! {
    static PREPARED_TRANSACTION_COUNT: Cell<u64> = const { Cell::new(0) };
}

/// An undo/redo history for committed graph transactions.
#[derive(Debug)]
pub struct GraphHistory {
    limit: usize,
    // Entries are normalized on insertion. `undo_len` is the boundary between applied and
    // unapplied entries, so history replay commits by moving only the boundary.
    transactions: VecDeque<GraphTransaction>,
    undo_len: usize,
}

impl Default for GraphHistory {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_LIMIT)
    }
}

impl GraphHistory {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            transactions: VecDeque::new(),
            undo_len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.transactions.clear();
        self.undo_len = 0;
    }

    #[cfg(test)]
    pub fn can_undo(&self) -> bool {
        self.undo_len > 0
    }

    #[cfg(test)]
    pub fn can_redo(&self) -> bool {
        self.undo_len < self.transactions.len()
    }

    pub fn undo_len(&self) -> usize {
        self.undo_len
    }

    pub fn redo_len(&self) -> usize {
        self.transactions.len() - self.undo_len
    }

    /// Records a committed transaction (original + derived concretization ops).
    pub fn record(&mut self, tx: GraphTransaction) {
        let tx = normalize_transaction(tx);
        if tx.is_empty() {
            return;
        }

        self.transactions.truncate(self.undo_len);
        if self.transactions.len() == self.limit {
            self.transactions.pop_front();
        }
        self.transactions.push_back(tx);
        self.undo_len = self.transactions.len();
    }

    /// Prepares the inverse of the latest applied transaction without changing history state.
    pub fn prepare_undo(&self) -> Option<GraphTransaction> {
        let transaction = self.transactions.get(self.undo_len.checked_sub(1)?)?;
        Self::record_prepared_transaction();
        Some(transaction.inverse())
    }

    /// Prepares the next unapplied transaction without changing history state.
    pub fn prepare_redo(&self) -> Option<GraphTransaction> {
        let transaction = self.transactions.get(self.undo_len)?;
        Self::record_prepared_transaction();
        Some(transaction.clone())
    }

    /// Commits a successful undo without allocating or inspecting transaction payloads.
    pub fn commit_undo(&mut self) {
        if self.undo_len > 0 {
            self.undo_len -= 1;
        }
    }

    /// Commits a successful redo without allocating or inspecting transaction payloads.
    pub fn commit_redo(&mut self) {
        if self.undo_len < self.transactions.len() {
            self.undo_len += 1;
        }
    }

    #[cfg(test)]
    pub(crate) fn reset_prepared_transaction_count_for_test() {
        PREPARED_TRANSACTION_COUNT.with(|count| count.set(0));
    }

    #[cfg(test)]
    pub(crate) fn prepared_transaction_count_for_test() -> u64 {
        PREPARED_TRANSACTION_COUNT.with(Cell::get)
    }

    fn record_prepared_transaction() {
        #[cfg(test)]
        PREPARED_TRANSACTION_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    }
}
