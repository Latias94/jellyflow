//! On-disk graph formats and versioned view-session seed state.

mod files;
mod view_state;

pub use files::{
    CanonicalGraphFileError, GRAPH_FILE_VERSION, GraphFile, GraphFileError, PersistenceLimitError,
    PersistenceLimits, PersistenceResource, VIEW_SESSION_SEED_FILE_VERSION, ViewSessionSeedFile,
    ViewSessionSeedFileError,
};

#[cfg(test)]
pub(crate) use files::{GraphFileSaveFault, ViewSessionSeedFileSaveFault};
pub(crate) use files::{PayloadCounter, validate_graph_resources};
pub use view_state::{
    ViewSelectionSeed, ViewSessionSeed, ViewSessionSeedGraphMismatch, ViewStackingSeed,
    ViewportSeed,
};
pub(crate) use view_state::{edge_is_visible, retain_first};

#[cfg(test)]
mod tests;
