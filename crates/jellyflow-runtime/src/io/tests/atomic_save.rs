use super::*;
use jellyflow_core::core::Graph;
#[cfg(unix)]
use jellyflow_core::core::{Symbol, SymbolId};

#[cfg(unix)]
use crate::runtime::commands::{GraphCommand, SymbolEdit};
#[cfg(unix)]
use crate::runtime::graph_runtime::GraphRuntime;
#[cfg(unix)]
use crate::schema::NodeCatalog;

#[test]
fn serialization_failure_preserves_the_last_valid_graph_file() {
    assert_save_fault_preserves_target(GraphFileSaveFault::Serialization);
}

#[test]
fn temporary_write_failure_preserves_the_last_valid_graph_file() {
    assert_save_fault_preserves_target(GraphFileSaveFault::TemporaryWrite);
}

#[test]
fn temporary_sync_failure_preserves_the_last_valid_graph_file() {
    assert_save_fault_preserves_target(GraphFileSaveFault::TemporarySync);
}

#[test]
fn reload_validation_failure_preserves_the_last_valid_graph_file() {
    assert_save_fault_preserves_target(GraphFileSaveFault::ReloadValidation);
}

#[test]
fn atomic_replace_failure_preserves_the_last_valid_graph_file() {
    assert_save_fault_preserves_target(GraphFileSaveFault::AtomicReplace);
}

#[test]
fn unknown_replace_outcome_exposes_and_preserves_the_staged_recovery_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let original_id = GraphId::from_u128(1);
    let replacement_id = GraphId::from_u128(2);
    GraphFile::from_graph(Graph::new(original_id))
        .save_json(&path)
        .unwrap();

    let error = GraphFile::from_graph(Graph::new(replacement_id))
        .save_json_with_fault(&path, GraphFileSaveFault::ReplaceOutcomeUnknown)
        .unwrap_err();

    let recovery_path = match error {
        GraphFileError::CommitOutcomeUnknown {
            recovery_path: Some(recovery_path),
            ..
        } => recovery_path,
        other => panic!("expected outcome-unknown recovery error, got {other:?}"),
    };
    assert_eq!(GraphFile::load_json(&path).unwrap().graph_id, original_id);
    assert_eq!(
        GraphFile::load_json(&recovery_path).unwrap().graph_id,
        replacement_id
    );
    assert_eq!(recovery_path.parent(), Some(directory.path()));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn atomic_save_requires_an_existing_parent_directory() {
    let root = tempfile::tempdir().expect("temporary root");
    let missing_parent = root.path().join("missing").join("nested");
    let path = missing_parent.join("graph.json");

    let error = GraphFile::from_graph(Graph::new(GraphId::new()))
        .save_json(&path)
        .expect_err("save must not create an unsynchronized directory chain");

    assert!(matches!(error, GraphFileError::Write { .. }));
    assert!(!missing_parent.exists());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn graph_overwrite_preserves_existing_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    GraphFile::from_graph(Graph::new(GraphId::from_u128(1)))
        .save_json(&path)
        .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();

    GraphFile::from_graph(Graph::new(GraphId::from_u128(2)))
        .save_json(&path)
        .unwrap();

    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn view_session_seed_overwrite_preserves_existing_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("view-session-seed.json");
    ViewSessionSeedFile::save_json(&ViewSessionSeed::new(GraphId::from_u128(1)), &path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    ViewSessionSeedFile::save_json(&ViewSessionSeed::new(GraphId::from_u128(2)), &path).unwrap();

    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[cfg(unix)]
#[test]
fn graph_overwrite_rejects_a_symlink_target_without_touching_its_referent() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let referent = directory.path().join("referent.json");
    let target = directory.path().join("graph.json");
    std::fs::write(&referent, b"referent bytes").unwrap();
    symlink(&referent, &target).unwrap();

    let error = GraphFile::from_graph(Graph::new(GraphId::from_u128(2)))
        .save_json(&target)
        .unwrap_err();

    assert!(matches!(error, GraphFileError::AtomicReplace { .. }));
    assert!(std::fs::symlink_metadata(&target).unwrap().is_symlink());
    assert_eq!(std::fs::read(&referent).unwrap(), b"referent bytes");
}

#[cfg(unix)]
#[test]
#[allow(
    unsafe_code,
    reason = "mkfifo has no stable Rust standard-library equivalent"
)]
fn graph_overwrite_rejects_a_fifo_without_opening_it() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileTypeExt;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("graph.json");
    let target_c = CString::new(target.as_os_str().as_bytes()).unwrap();
    let created = unsafe { libc::mkfifo(target_c.as_ptr(), 0o600) };
    assert_eq!(
        created,
        0,
        "create FIFO fixture: {}",
        std::io::Error::last_os_error()
    );

    let error = GraphFile::from_graph(Graph::new(GraphId::from_u128(2)))
        .save_json(&target)
        .unwrap_err();

    assert!(matches!(error, GraphFileError::AtomicReplace { .. }));
    assert!(
        std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[cfg(windows)]
#[test]
fn graph_overwrite_preserves_existing_windows_dacl() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    GraphFile::from_graph(Graph::new(GraphId::from_u128(1)))
        .save_json(&path)
        .unwrap();
    protect_current_dacl(&path);
    let dacl_before = dacl_sddl(&path);
    assert!(
        dacl_before.starts_with("D:P"),
        "the fixture requires a protected target DACL: {dacl_before}"
    );

    GraphFile::from_graph(Graph::new(GraphId::from_u128(2)))
        .save_json(&path)
        .unwrap();

    assert_eq!(dacl_sddl(&path), dacl_before);
    assert_eq!(
        GraphFile::load_json(&path).unwrap().graph_id,
        GraphId::from_u128(2)
    );
}

#[cfg(unix)]
#[test]
fn parent_directory_open_failure_happens_after_replacement_and_leaves_no_stage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let replacement_id = GraphId::from_u128(2);
    GraphFile::from_graph(Graph::new(GraphId::from_u128(1)))
        .save_json(&path)
        .unwrap();

    let error = GraphFile::from_graph(Graph::new(replacement_id))
        .save_json_with_fault(&path, GraphFileSaveFault::ParentDirectoryOpen)
        .unwrap_err();

    match error {
        GraphFileError::CommitDurabilityUnknown { recovery_path, .. } => {
            assert_eq!(recovery_path, None);
        }
        other => panic!("expected durability-unknown error, got {other:?}"),
    }
    assert_eq!(
        GraphFile::load_json(&path).unwrap().graph_id,
        replacement_id
    );
    assert_eq!(
        directory_entries(directory.path()),
        vec!["graph.json".to_owned()]
    );
}

#[cfg(unix)]
#[test]
fn parent_directory_sync_failure_recovers_without_mutating_the_live_runtime() {
    let graph_id = GraphId::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");

    GraphFile::from_graph(Graph::new(graph_id))
        .save_json(&path)
        .unwrap();
    let catalog = NodeCatalog::new();
    let live_runtime =
        GraphRuntime::from_canonical(GraphFile::load_canonical_json(&path, &catalog).unwrap())
            .unwrap();
    let live_symbol = SymbolId::from_u128(0x51);
    let live_version = live_runtime.snapshot().unwrap().version();
    live_runtime
        .execute(GraphCommand::symbols(
            live_version,
            SymbolEdit::new().add(
                live_symbol,
                Symbol {
                    name: "live-only".to_owned(),
                    ty: None,
                    default_value: Some(serde_json::json!(true)),
                    meta: serde_json::json!({ "source": "runtime" }),
                },
            ),
        ))
        .unwrap()
        .applied()
        .expect("live runtime mutation applies");
    let live_snapshot_before = live_runtime.snapshot().unwrap();
    let live_history_before = live_runtime.history_state().unwrap();
    assert_eq!(live_snapshot_before.revision(), 1);
    assert_eq!(live_history_before.undo_len(), 1);

    let replacement_id = GraphId::new();
    let replacement = GraphFile::from_graph(Graph::new(replacement_id));

    let err = replacement
        .save_json_with_fault(&path, GraphFileSaveFault::ParentDirectorySync)
        .unwrap_err();

    match &err {
        GraphFileError::CommitDurabilityUnknown { recovery_path, .. } => {
            assert_eq!(recovery_path, &None);
        }
        other => panic!("expected durability-unknown error, got {other:?}"),
    }
    let recovered_runtime = GraphRuntime::from_canonical(
        GraphFile::load_canonical_json(&path, &catalog)
            .expect("recovery reload validates one complete canonical graph"),
    )
    .expect("recovery publishes a separate runtime only after validation");
    let recovered = recovered_runtime.snapshot().unwrap();
    assert_eq!(recovered.graph().graph_id(), replacement_id);
    assert_eq!(recovered.revision(), 0);
    assert_eq!(recovered_runtime.history_state().unwrap().undo_len(), 0);

    let live_snapshot_after = live_runtime.snapshot().unwrap();
    assert!(
        live_snapshot_before.ptr_eq(&live_snapshot_after),
        "a failed save and explicit recovery reload must not replace live runtime state"
    );
    assert_eq!(live_runtime.history_state().unwrap(), live_history_before);
    assert_eq!(live_snapshot_after.graph().graph_id(), graph_id);
    assert!(live_snapshot_after.graph().symbol(&live_symbol).is_some());
    assert_eq!(
        directory_entries(directory.path()),
        vec!["graph.json".to_owned()]
    );
}

fn assert_save_fault_preserves_target(fault: GraphFileSaveFault) {
    let graph_id = GraphId::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");

    let original = GraphFile::from_graph(Graph::new(graph_id));
    original.save_json(&path).unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let replacement_id = GraphId::new();
    let replacement = GraphFile::from_graph(Graph::new(replacement_id));
    let err = replacement.save_json_with_fault(&path, fault).unwrap_err();

    assert_ne!(
        replacement_id, graph_id,
        "fixture must change persisted facts"
    );
    assert_fault_error(fault, &err);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(GraphFile::load_json(&path).unwrap().graph_id, graph_id);
    assert_eq!(
        directory_entries(directory.path()),
        vec!["graph.json".to_owned()]
    );
}

fn assert_fault_error(fault: GraphFileSaveFault, err: &GraphFileError) {
    match fault {
        GraphFileSaveFault::Serialization => {
            assert!(matches!(err, GraphFileError::Serialize { .. }));
        }
        GraphFileSaveFault::TemporaryWrite => {
            assert!(matches!(err, GraphFileError::Write { .. }));
        }
        GraphFileSaveFault::TemporarySync => {
            assert!(matches!(err, GraphFileError::StageSync { .. }));
        }
        GraphFileSaveFault::ReloadValidation => {
            assert!(matches!(err, GraphFileError::ReloadValidation { .. }));
        }
        GraphFileSaveFault::AtomicReplace => {
            assert!(matches!(err, GraphFileError::AtomicReplace { .. }));
        }
        GraphFileSaveFault::None
        | GraphFileSaveFault::ReplaceOutcomeUnknown
        | GraphFileSaveFault::ParentDirectoryOpen
        | GraphFileSaveFault::ParentDirectorySync => {
            panic!("the test requires a pre-commit fault")
        }
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the Windows regression fixture must protect and inspect a real filesystem DACL"
)]
fn protect_current_dacl(path: &std::path::Path) {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SE_FILE_OBJECT, SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };

    let wide_path = path_to_wide(path);
    let mut dacl: *mut ACL = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, ERROR_SUCCESS, "read target DACL");
    let descriptor = LocalAllocation(descriptor);

    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null_mut(),
        )
    };
    assert_eq!(status, ERROR_SUCCESS, "protect target DACL");
    drop(descriptor);
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "the Windows regression fixture must read a real filesystem DACL as SDDL"
)]
fn dacl_sddl(path: &std::path::Path) -> String {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};

    let wide_path = path_to_wide(path);
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, ERROR_SUCCESS, "read target security descriptor");
    let descriptor = LocalAllocation(descriptor);

    let mut string_descriptor = null_mut();
    let mut string_len = 0;
    let converted = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut string_descriptor,
            &mut string_len,
        )
    };
    assert_ne!(converted, 0, "convert target DACL to SDDL");
    let string_descriptor = LocalAllocation(string_descriptor.cast());
    let wide = unsafe {
        std::slice::from_raw_parts(string_descriptor.0.cast::<u16>(), string_len as usize)
    };
    String::from_utf16(wide).expect("Windows SDDL is valid UTF-16")
}

#[cfg(windows)]
fn path_to_wide(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
struct LocalAllocation(*mut std::ffi::c_void);

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "LocalFree is required for buffers allocated by Windows security APIs"
)]
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::LocalFree;

        if !self.0.is_null() {
            let _ = unsafe { LocalFree(self.0) };
        }
    }
}
