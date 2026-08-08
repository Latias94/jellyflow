use super::*;

pub(super) struct FileFixtureMigrator;

impl NodeKindMigrator for FileFixtureMigrator {
    fn migrate(
        &self,
        _from_version: u32,
        _to_version: u32,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        Ok(serde_json::json!({ "previous": data, "canonical": true }))
    }
}

pub(super) struct CountingFileFixtureMigrator {
    pub(super) calls: Arc<AtomicUsize>,
}

impl NodeKindMigrator for CountingFileFixtureMigrator {
    fn migrate(
        &self,
        _from_version: u32,
        _to_version: u32,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(data.clone())
    }
}

pub(super) struct CumulativeOutputExceedsBudgetMigrator {
    pub(super) calls: Arc<AtomicUsize>,
}

pub(super) const REMOVED_FOCUSABILITY_KEY: &str = "focusable";

impl NodeKindMigrator for CumulativeOutputExceedsBudgetMigrator {
    fn migrate(
        &self,
        _from_version: u32,
        _to_version: u32,
        _data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            call < 2,
            "a later migrator ran after cumulative outputs exceeded their budget"
        );
        Ok(serde_json::json!([null]))
    }
}

pub(super) fn file_fixture_node(kind: &str, kind_version: u32, data: serde_json::Value) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version,
        pos: CanvasPoint::default(),
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data,
    }
}

pub(super) fn file_fixture_port(
    node: NodeId,
    key: &str,
    direction: PortDirection,
    capacity: PortCapacity,
) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(TypeDesc::Float),
        data: serde_json::Value::Null,
    }
}

pub(super) fn graph_fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/io/tests/fixtures")
        .join(name)
}

pub(super) fn load_corrupt_graph_fixture(
    name: &str,
    mutate: impl FnOnce(&mut serde_json::Value),
) -> Vec<GraphValidationError> {
    let fixture_path = graph_fixture_path("graph-file-v2-canonical.json");
    let mut source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture_path).unwrap()).unwrap();
    mutate(&mut source);

    let path = temp_path(name, GraphId::new());
    std::fs::write(&path, serde_json::to_vec_pretty(&source).unwrap()).unwrap();
    let error = GraphFile::load_json(&path).unwrap_err();
    let _ = std::fs::remove_file(&path);

    match error {
        GraphFileError::StorageValidation { errors } => errors,
        other => panic!("expected storage validation failure, got {other}"),
    }
}
