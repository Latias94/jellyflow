use crate::ops::{GraphTransaction, normalize_transaction};

/// Maximum number of transactions retained by default.
pub const DEFAULT_HISTORY_LIMIT: usize = 256;

/// A simple undo/redo history for committed graph transactions.
#[derive(Debug, Clone)]
pub struct GraphHistory {
    limit: usize,
    undo: Vec<GraphTransaction>,
    redo: Vec<GraphTransaction>,
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
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Records a committed transaction (original + derived concretization ops).
    pub fn record(&mut self, tx: GraphTransaction) {
        let tx = normalize_transaction(tx);
        if tx.ops.is_empty() {
            return;
        }
        self.undo.push(tx);
        self.redo.clear();
        if self.undo.len() > self.limit {
            let overflow = self.undo.len() - self.limit;
            self.undo.drain(0..overflow);
        }
    }

    /// Undoes the last recorded transaction by applying its inverse transaction.
    ///
    /// The `apply` closure is responsible for applying the transaction to the graph and returning
    /// the committed transaction (including any derived ops produced by the profile pipeline).
    pub fn undo<E>(
        &mut self,
        mut apply: impl FnMut(&GraphTransaction) -> Result<GraphTransaction, E>,
    ) -> Result<bool, E> {
        let Some(tx) = self.undo.pop() else {
            return Ok(false);
        };

        let inverse = tx.inverse();
        match apply(&inverse) {
            Ok(committed) => {
                let redo_tx = committed.inverse();
                self.redo.push(redo_tx);
                Ok(true)
            }
            Err(err) => {
                self.undo.push(tx);
                Err(err)
            }
        }
    }

    /// Redoes the last undone transaction.
    pub fn redo<E>(
        &mut self,
        mut apply: impl FnMut(&GraphTransaction) -> Result<GraphTransaction, E>,
    ) -> Result<bool, E> {
        let Some(tx) = self.redo.pop() else {
            return Ok(false);
        };

        match apply(&tx) {
            Ok(committed) => {
                self.undo.push(committed);
                Ok(true)
            }
            Err(err) => {
                self.redo.push(tx);
                Err(err)
            }
        }
    }
}
