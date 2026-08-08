use std::collections::VecDeque;

use crate::graph_kernel::{GraphTransaction, normalize_transaction};

/// Maximum number of transactions retained by default.
pub const DEFAULT_HISTORY_LIMIT: usize = 256;

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
        Some(transaction.inverse())
    }

    /// Prepares the next unapplied transaction without changing history state.
    pub fn prepare_redo(&self) -> Option<GraphTransaction> {
        let transaction = self.transactions.get(self.undo_len)?;
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
}
