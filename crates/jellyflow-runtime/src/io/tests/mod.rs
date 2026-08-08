use std::path::PathBuf;

use jellyflow_core::core::GraphId;

use super::*;

mod atomic_save;
mod graph_file;
mod view_session_seed_file;

fn temp_path(name: &str, graph_id: GraphId) -> PathBuf {
    std::env::temp_dir().join(format!("jellyflow_runtime_{name}_{graph_id}.json"))
}

fn directory_entries(directory: &std::path::Path) -> Vec<String> {
    let mut entries = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .file_name()
                .into_string()
                .expect("test paths are UTF-8")
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
