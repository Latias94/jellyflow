//! Graph and view-session seed persistence formats.

mod atomic_save;
mod graph;
mod limits;
mod view_session_seed;

pub use graph::{CanonicalGraphFileError, GRAPH_FILE_VERSION, GraphFile, GraphFileError};
pub(crate) use limits::{PayloadCounter, validate_graph_resources};
pub use limits::{PersistenceLimitError, PersistenceLimits, PersistenceResource};
pub use view_session_seed::{
    VIEW_SESSION_SEED_FILE_VERSION, ViewSessionSeedFile, ViewSessionSeedFileError,
};

#[cfg(test)]
pub(crate) use graph::GraphFileSaveFault;
#[cfg(test)]
pub(crate) use view_session_seed::ViewSessionSeedFileSaveFault;
