pub(super) fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "jellyflow-conformance-{name}-{}.json",
        uuid::Uuid::new_v4()
    ))
}

pub(super) fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "jellyflow-conformance-{name}-{}",
        uuid::Uuid::new_v4()
    ))
}
