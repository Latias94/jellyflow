use std::path::PathBuf;

use jellyflow_core::core::GraphId;

use super::*;

mod editor_state_file;
mod graph_file;
mod interaction;
mod view_state;

fn temp_path(name: &str, graph_id: GraphId) -> PathBuf {
    std::env::temp_dir().join(format!("jellyflow_runtime_{name}_{graph_id}.json"))
}
