use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use tempfile::{NamedTempFile, TempPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AtomicSaveFault {
    None,
    #[cfg(test)]
    Serialization,
    TemporaryWrite,
    ReloadValidation,
    TemporarySync,
    AtomicReplace,
    #[cfg(test)]
    ReplaceOutcomeUnknown,
    ParentDirectoryOpen,
    ParentDirectorySync,
}

#[derive(Debug)]
pub(super) enum AtomicSaveError<EncodeError, ReloadValidationError> {
    Encode(EncodeError),
    Write(io::Error),
    ReloadValidation(ReloadValidationError),
    StageSync(io::Error),
    NotCommitted(io::Error),
    OutcomeUnknown {
        source: io::Error,
        recovery_path: Option<PathBuf>,
    },
    DurabilityUnknown {
        source: io::Error,
        recovery_path: Option<PathBuf>,
    },
}

#[derive(Debug)]
enum ReplaceFailure {
    NotCommitted(io::Error),
    #[cfg_attr(
        not(windows),
        allow(
            dead_code,
            reason = "only Win32 replacement can report a partial outcome"
        )
    )]
    OutcomeUnknown(io::Error),
}

pub(super) fn save_atomically<EncodeError, ReloadValidationError>(
    target: &Path,
    temporary_prefix: &str,
    fault: AtomicSaveFault,
    encode: impl FnOnce() -> Result<Vec<u8>, EncodeError>,
    validate_reload: impl FnOnce(&[u8]) -> Result<(), ReloadValidationError>,
) -> Result<(), AtomicSaveError<EncodeError, ReloadValidationError>> {
    let parent = parent_directory(target);
    ensure_directory(parent).map_err(AtomicSaveError::Write)?;
    let existing_target = existing_target(target).map_err(AtomicSaveError::NotCommitted)?;

    let bytes = encode().map_err(AtomicSaveError::Encode)?;
    let mut staged = create_staged_file(parent, temporary_prefix, existing_target.as_ref())
        .map_err(AtomicSaveError::Write)?;

    if fault == AtomicSaveFault::TemporaryWrite {
        return Err(AtomicSaveError::Write(io::Error::other(
            "injected temporary-write failure",
        )));
    }
    staged
        .as_file_mut()
        .write_all(&bytes)
        .and_then(|()| staged.as_file_mut().rewind())
        .map_err(AtomicSaveError::Write)?;
    let staged_capacity = bytes.len();
    drop(bytes);

    if fault == AtomicSaveFault::ReloadValidation {
        staged
            .as_file_mut()
            .set_len(0)
            .and_then(|()| staged.as_file_mut().rewind())
            .map_err(AtomicSaveError::Write)?;
    }
    let mut staged_bytes = Vec::with_capacity(staged_capacity);
    staged
        .as_file_mut()
        .read_to_end(&mut staged_bytes)
        .map_err(AtomicSaveError::Write)?;
    validate_reload(&staged_bytes).map_err(AtomicSaveError::ReloadValidation)?;

    if fault == AtomicSaveFault::TemporarySync {
        return Err(AtomicSaveError::StageSync(io::Error::other(
            "injected staged-file sync failure",
        )));
    }
    if let Some(existing_target) = existing_target.as_ref() {
        existing_target
            .apply_to(staged.as_file())
            .map_err(AtomicSaveError::NotCommitted)?;
    }
    staged
        .as_file()
        .sync_all()
        .map_err(AtomicSaveError::StageSync)?;

    replace_staged_file(
        staged.into_temp_path(),
        target,
        existing_target.is_some(),
        fault,
    )
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    let metadata = std::fs::metadata(path)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("atomic-save parent is not a directory: {}", path.display()),
        ))
    }
}

struct ExistingTarget {
    permissions: std::fs::Permissions,
    #[cfg(unix)]
    acl: unix_acl::TargetAcl,
    #[cfg(windows)]
    dacl: windows::TargetDacl,
}

impl ExistingTarget {
    fn apply_to(&self, staged: &std::fs::File) -> io::Result<()> {
        staged.set_permissions(self.permissions.clone())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            self.acl.apply_to(staged)?;
            let actual_mode = staged.metadata()?.permissions().mode() & 0o7777;
            let expected_mode = self.permissions.mode() & 0o7777;
            if actual_mode != expected_mode {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "staged file mode changed while restoring ACL: expected {expected_mode:#06o}, got {actual_mode:#06o}"
                    ),
                ));
            }
            self.acl.verify(staged)?;
        }

        Ok(())
    }
}

struct StagedFile {
    // Field order is load-bearing on Windows: close the exclusive handle before removing the path.
    file: std::fs::File,
    path: TempPath,
}

impl StagedFile {
    fn from_named(staged: NamedTempFile) -> Self {
        let (file, path) = staged.into_parts();
        Self { file, path }
    }

    fn as_file(&self) -> &std::fs::File {
        &self.file
    }

    fn as_file_mut(&mut self) -> &mut std::fs::File {
        &mut self.file
    }

    fn into_temp_path(self) -> TempPath {
        let Self { file, path } = self;
        drop(file);
        path
    }

    #[cfg(all(test, windows))]
    fn path(&self) -> &Path {
        self.path.as_ref()
    }
}

#[cfg(unix)]
fn existing_target(target: &Path) -> io::Result<Option<ExistingTarget>> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let path_metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(source),
    };
    if !path_metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "atomic-save target must be a regular file and must not be a symlink: {}",
                target.display()
            ),
        ));
    }

    let opened = OpenOptions::new()
        .read(true)
        .custom_flags(existing_target_open_flags())
        .open(target)?;
    let opened_metadata = opened.metadata()?;
    if !opened_metadata.is_file()
        || path_metadata.dev() != opened_metadata.dev()
        || path_metadata.ino() != opened_metadata.ino()
    {
        return Err(io::Error::other(format!(
            "atomic-save target changed while its metadata was captured: {}",
            target.display()
        )));
    }

    Ok(Some(ExistingTarget {
        permissions: opened_metadata.permissions(),
        acl: unix_acl::TargetAcl::read(&opened)?,
    }))
}

#[cfg(unix)]
fn existing_target_open_flags() -> i32 {
    libc::O_NOFOLLOW | libc::O_NONBLOCK
}

#[cfg(not(unix))]
fn existing_target(target: &Path) -> io::Result<Option<ExistingTarget>> {
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_file() => Ok(Some(ExistingTarget {
            permissions: metadata.permissions(),
            #[cfg(windows)]
            dacl: windows::TargetDacl::read(target)?,
        })),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "atomic-save target must be a regular file and must not be a symlink: {}",
                target.display()
            ),
        )),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(source),
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
mod unix_acl {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io;

    use xattr::FileExt;

    const ACCESS_ACL: &str = "system.posix_acl_access";

    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct TargetAcl(Option<Vec<u8>>);

    impl TargetAcl {
        pub(super) fn read(file: &File) -> io::Result<Self> {
            file.get_xattr(OsStr::new(ACCESS_ACL)).map(Self)
        }

        pub(super) fn apply_to(&self, file: &File) -> io::Result<()> {
            match self.0.as_deref() {
                Some(acl) => file.set_xattr(OsStr::new(ACCESS_ACL), acl),
                None => match file.remove_xattr(OsStr::new(ACCESS_ACL)) {
                    Ok(()) => Ok(()),
                    Err(source) if remove_error_is_absent(&source) => Ok(()),
                    Err(source) => Err(source),
                },
            }
        }

        pub(super) fn verify(&self, file: &File) -> io::Result<()> {
            let actual = Self::read(file)?;
            if actual == *self {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "staged file ACL does not match the existing target ACL",
                ))
            }
        }

        #[cfg(test)]
        pub(super) fn has_extended_entries(&self) -> bool {
            self.0.is_some()
        }
    }

    fn remove_error_is_absent(error: &io::Error) -> bool {
        error.raw_os_error() == Some(libc::ENODATA)
    }

    #[cfg(test)]
    mod tests {
        use std::io;

        use super::remove_error_is_absent;

        #[test]
        fn only_enodata_means_the_access_acl_is_absent() {
            assert!(remove_error_is_absent(&io::Error::from_raw_os_error(
                libc::ENODATA
            )));
            for errno in [libc::ENOTSUP, libc::EOPNOTSUPP, libc::ENOSYS] {
                assert!(
                    !remove_error_is_absent(&io::Error::from_raw_os_error(errno)),
                    "errno {errno} must fail closed"
                );
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "macOS ACLs require libc ACL APIs and have no safe standard-library equivalent"
)]
mod unix_acl {
    use std::ffi::{c_int, c_uint, c_void};
    use std::fs::File;
    use std::io;
    use std::os::fd::AsRawFd;
    use std::ptr::null_mut;

    const ACL_TYPE_EXTENDED: c_uint = 256;
    const ACL_FIRST_ENTRY: c_int = 0;

    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct TargetAcl(Option<Vec<u8>>);

    impl TargetAcl {
        pub(super) fn read(file: &File) -> io::Result<Self> {
            let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
            if acl.is_null() {
                let source = io::Error::last_os_error();
                return if get_error_means_no_acl(&source) {
                    Ok(Self(None))
                } else {
                    Err(source)
                };
            }
            let acl = OwnedAcl(acl);
            if acl.is_empty()? {
                Ok(Self(None))
            } else {
                Ok(Self(Some(acl.external_bytes()?)))
            }
        }

        pub(super) fn apply_to(&self, file: &File) -> io::Result<()> {
            let acl = match self.0.as_deref() {
                Some(bytes) => {
                    let acl = unsafe { acl_copy_int(bytes.as_ptr().cast()) };
                    if acl.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    OwnedAcl(acl)
                }
                None => {
                    let acl = unsafe { acl_init(0) };
                    if acl.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    OwnedAcl(acl)
                }
            };

            let result = unsafe { acl_set_fd_np(file.as_raw_fd(), acl.0, ACL_TYPE_EXTENDED) };
            if result == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }

        pub(super) fn verify(&self, file: &File) -> io::Result<()> {
            let actual = Self::read(file)?;
            if actual == *self {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "staged file ACL does not match the existing target ACL",
                ))
            }
        }

        #[cfg(test)]
        pub(super) fn has_extended_entries(&self) -> bool {
            self.0.is_some()
        }
    }

    struct OwnedAcl(*mut c_void);

    impl OwnedAcl {
        fn is_empty(&self) -> io::Result<bool> {
            let mut entry = null_mut();
            match unsafe { acl_get_entry(self.0, ACL_FIRST_ENTRY, &mut entry) } {
                0 => Ok(false),
                _ => {
                    let source = io::Error::last_os_error();
                    if source.raw_os_error() == Some(libc::EINVAL) {
                        Ok(true)
                    } else {
                        Err(source)
                    }
                }
            }
        }

        fn external_bytes(&self) -> io::Result<Vec<u8>> {
            let size = unsafe { acl_size(self.0) };
            let size = usize::try_from(size).map_err(|_| io::Error::last_os_error())?;
            let mut bytes = vec![0; size];
            let written = unsafe { acl_copy_ext(bytes.as_mut_ptr().cast(), self.0, size as isize) };
            let written = usize::try_from(written).map_err(|_| io::Error::last_os_error())?;
            bytes.truncate(written);
            Ok(bytes)
        }
    }

    impl Drop for OwnedAcl {
        fn drop(&mut self) {
            let _ = unsafe { acl_free(self.0) };
        }
    }

    fn get_error_means_no_acl(error: &io::Error) -> bool {
        error.raw_os_error() == Some(libc::ENOENT)
    }

    #[cfg(test)]
    mod tests {
        use std::io;

        use super::get_error_means_no_acl;

        #[test]
        fn only_enoent_means_the_extended_acl_is_absent() {
            assert!(get_error_means_no_acl(&io::Error::from_raw_os_error(
                libc::ENOENT
            )));
            for errno in [libc::ENOTSUP, libc::EOPNOTSUPP, libc::ENOSYS] {
                assert!(
                    !get_error_means_no_acl(&io::Error::from_raw_os_error(errno)),
                    "errno {errno} must fail closed"
                );
            }
        }
    }

    unsafe extern "C" {
        fn acl_copy_ext(buffer: *mut c_void, acl: *mut c_void, size: isize) -> isize;
        fn acl_copy_int(buffer: *const c_void) -> *mut c_void;
        fn acl_free(object: *mut c_void) -> c_int;
        fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
        fn acl_get_fd_np(fd: c_int, acl_type: c_uint) -> *mut c_void;
        fn acl_init(count: c_int) -> *mut c_void;
        fn acl_set_fd_np(fd: c_int, acl: *mut c_void, acl_type: c_uint) -> c_int;
        fn acl_size(acl: *mut c_void) -> isize;
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "android", target_os = "linux", target_os = "macos"))
))]
mod unix_acl {
    use std::fs::File;
    use std::io;

    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct TargetAcl;

    impl TargetAcl {
        pub(super) fn read(_file: &File) -> io::Result<Self> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ACL-preserving atomic overwrite is not implemented for this Unix platform",
            ))
        }

        pub(super) fn apply_to(&self, _file: &File) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ACL-preserving atomic overwrite is not implemented for this Unix platform",
            ))
        }

        pub(super) fn verify(&self, _file: &File) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ACL-preserving atomic overwrite is not implemented for this Unix platform",
            ))
        }

        #[cfg(test)]
        pub(super) fn has_extended_entries(&self) -> bool {
            false
        }
    }
}

#[cfg(not(windows))]
fn create_staged_file(
    parent: &Path,
    temporary_prefix: &str,
    _existing_target: Option<&ExistingTarget>,
) -> io::Result<StagedFile> {
    tempfile::Builder::new()
        .prefix(temporary_prefix)
        .suffix(".tmp")
        .tempfile_in(parent)
        .map(StagedFile::from_named)
}

#[cfg(windows)]
fn create_staged_file(
    parent: &Path,
    temporary_prefix: &str,
    existing_target: Option<&ExistingTarget>,
) -> io::Result<StagedFile> {
    windows::create_staged_file(
        parent,
        temporary_prefix,
        existing_target.map(|target| &target.dacl),
    )
}

fn replace_staged_file<EncodeError, ReloadValidationError>(
    mut staged: TempPath,
    target: &Path,
    target_existed: bool,
    fault: AtomicSaveFault,
) -> Result<(), AtomicSaveError<EncodeError, ReloadValidationError>> {
    if fault == AtomicSaveFault::AtomicReplace {
        return Err(AtomicSaveError::NotCommitted(io::Error::other(
            "injected atomic-replace failure",
        )));
    }
    #[cfg(test)]
    if fault == AtomicSaveFault::ReplaceOutcomeUnknown {
        let recovery_path = preserve_staged_for_recovery(staged);
        return Err(AtomicSaveError::OutcomeUnknown {
            source: io::Error::other("injected replacement-outcome-unknown failure"),
            recovery_path,
        });
    }

    match replace_file(staged.as_ref(), target, target_existed) {
        Ok(()) => {}
        Err(ReplaceFailure::NotCommitted(source)) => {
            return Err(AtomicSaveError::NotCommitted(source));
        }
        Err(ReplaceFailure::OutcomeUnknown(source)) => {
            let recovery_path = preserve_staged_for_recovery(staged);
            return Err(AtomicSaveError::OutcomeUnknown {
                source,
                recovery_path,
            });
        }
    }
    staged.disable_cleanup(true);

    sync_parent_after_replace(target, fault)
}

fn preserve_staged_for_recovery(mut staged: TempPath) -> Option<PathBuf> {
    let staged_path: &Path = staged.as_ref();
    let path = staged_path.to_path_buf();
    staged.disable_cleanup(true);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => Some(path),
        Err(source) if source.kind() == io::ErrorKind::NotFound => None,
        Err(_) => Some(path),
    }
}

fn parent_directory(target: &Path) -> &Path {
    target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(unix)]
fn replace_file(staged: &Path, target: &Path, _target_existed: bool) -> Result<(), ReplaceFailure> {
    std::fs::rename(staged, target).map_err(ReplaceFailure::NotCommitted)
}

#[cfg(windows)]
fn replace_file(staged: &Path, target: &Path, target_existed: bool) -> Result<(), ReplaceFailure> {
    if target_existed {
        windows::replace_existing_file(staged, target)
    } else {
        windows::move_new_file(staged, target)
    }
}

#[cfg(not(any(unix, windows)))]
fn replace_file(staged: &Path, target: &Path, _target_existed: bool) -> Result<(), ReplaceFailure> {
    std::fs::rename(staged, target).map_err(ReplaceFailure::NotCommitted)
}

#[cfg(unix)]
fn sync_parent_after_replace<EncodeError, ReloadValidationError>(
    target: &Path,
    fault: AtomicSaveFault,
) -> Result<(), AtomicSaveError<EncodeError, ReloadValidationError>> {
    let directory = open_parent_directory(parent_directory(target), fault).map_err(|source| {
        AtomicSaveError::DurabilityUnknown {
            source,
            recovery_path: None,
        }
    })?;
    sync_parent_directory(&directory, fault).map_err(|source| AtomicSaveError::DurabilityUnknown {
        source,
        recovery_path: None,
    })
}

#[cfg(unix)]
fn open_parent_directory(path: &Path, fault: AtomicSaveFault) -> io::Result<std::fs::File> {
    if fault == AtomicSaveFault::ParentDirectoryOpen {
        return Err(io::Error::other("injected parent-directory open failure"));
    }
    std::fs::File::open(path)
}

#[cfg(unix)]
fn sync_parent_directory(directory: &std::fs::File, fault: AtomicSaveFault) -> io::Result<()> {
    if fault == AtomicSaveFault::ParentDirectorySync {
        return Err(io::Error::other("injected parent-directory sync failure"));
    }
    directory.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_after_replace<EncodeError, ReloadValidationError>(
    _target: &Path,
    fault: AtomicSaveFault,
) -> Result<(), AtomicSaveError<EncodeError, ReloadValidationError>> {
    let source = if fault == AtomicSaveFault::ParentDirectoryOpen {
        Some(io::Error::other("injected parent-directory open failure"))
    } else if fault == AtomicSaveFault::ParentDirectorySync {
        Some(io::Error::other("injected parent-directory sync failure"))
    } else {
        None
    };
    match source {
        Some(source) => Err(AtomicSaveError::DurabilityUnknown {
            source,
            recovery_path: None,
        }),
        None => Ok(()),
    }
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "Win32 DACL and atomic replacement APIs have no safe standard-library equivalents"
)]
mod windows {
    use std::fs::{File, OpenOptions};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::ptr::{null, null_mut};
    use std::{io, iter};

    use super::{ReplaceFailure, StagedFile};
    use windows_sys::Win32::Foundation::{
        ERROR_SUCCESS, ERROR_UNABLE_TO_MOVE_REPLACEMENT, ERROR_UNABLE_TO_MOVE_REPLACEMENT_2,
        LocalFree,
    };
    #[cfg(test)]
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SE_FILE_OBJECT, SetSecurityInfo,
    };
    use windows_sys::Win32::Security::{
        ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
        WRITE_DAC,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum ReplaceOperation {
        ExistingTarget,
        NewTarget,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum FailureClass {
        NotCommitted,
        OutcomeUnknown,
    }

    pub(super) struct TargetDacl {
        descriptor: PSECURITY_DESCRIPTOR,
        dacl: *mut ACL,
    }

    impl TargetDacl {
        pub(super) fn read(path: &Path) -> io::Result<Self> {
            let path = path_to_wide(path);
            let mut dacl = null_mut();
            let mut descriptor = null_mut();
            let status = unsafe {
                GetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    &mut dacl,
                    null_mut(),
                    &mut descriptor,
                )
            };
            win32_status(status)?;
            Ok(Self { descriptor, dacl })
        }

        pub(super) fn apply_to(&self, file: &File) -> io::Result<()> {
            let status = unsafe {
                SetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    self.dacl,
                    null_mut(),
                )
            };
            win32_status(status)
        }

        #[cfg(test)]
        pub(super) fn sddl(&self) -> io::Result<String> {
            let mut string_descriptor = null_mut();
            let mut string_len = 0;
            let converted = unsafe {
                ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    self.descriptor,
                    SDDL_REVISION_1,
                    DACL_SECURITY_INFORMATION,
                    &mut string_descriptor,
                    &mut string_len,
                )
            };
            bool_result(converted)?;
            let string_descriptor = LocalWideString(string_descriptor);
            let wide =
                unsafe { std::slice::from_raw_parts(string_descriptor.0, string_len as usize) };
            let wide = wide.strip_suffix(&[0]).unwrap_or(wide);
            String::from_utf16(wide).map_err(|source| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Windows returned invalid UTF-16 SDDL: {source}"),
                )
            })
        }
    }

    impl Drop for TargetDacl {
        fn drop(&mut self) {
            if !self.descriptor.is_null() {
                let _ = unsafe { LocalFree(self.descriptor) };
            }
        }
    }

    #[cfg(test)]
    struct LocalWideString(*mut u16);

    #[cfg(test)]
    impl Drop for LocalWideString {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let _ = unsafe { LocalFree(self.0.cast()) };
            }
        }
    }

    pub(super) fn create_staged_file(
        parent: &Path,
        temporary_prefix: &str,
        target_dacl: Option<&TargetDacl>,
    ) -> io::Result<StagedFile> {
        let staged = tempfile::Builder::new()
            .prefix(temporary_prefix)
            .suffix(".tmp")
            .make_in(parent, |path| {
                let mut options = OpenOptions::new();
                options
                    .create_new(true)
                    .read(true)
                    .write(true)
                    .share_mode(0);
                if target_dacl.is_some() {
                    options.access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC);
                }
                options.open(path)
            })?;
        let staged = StagedFile::from_named(staged);
        if let Some(target_dacl) = target_dacl {
            target_dacl.apply_to(staged.as_file())?;
        }
        // A new target uses its parent-inherited DACL as both staged and final policy.
        Ok(staged)
    }

    pub(super) fn replace_existing_file(
        staged: &Path,
        target: &Path,
    ) -> Result<(), ReplaceFailure> {
        let staged = path_to_wide(staged);
        let target = path_to_wide(target);
        let replaced =
            unsafe { ReplaceFileW(target.as_ptr(), staged.as_ptr(), null(), 0, null(), null()) };
        replace_result(replaced, ReplaceOperation::ExistingTarget)
    }

    pub(super) fn move_new_file(staged: &Path, target: &Path) -> Result<(), ReplaceFailure> {
        let staged = path_to_wide(staged);
        let target = path_to_wide(target);
        let moved =
            unsafe { MoveFileExW(staged.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) };
        replace_result(moved, ReplaceOperation::NewTarget)
    }

    pub(super) fn classify_failure(
        operation: ReplaceOperation,
        raw_os_error: Option<i32>,
    ) -> FailureClass {
        match operation {
            ReplaceOperation::ExistingTarget
                if matches!(
                    raw_os_error,
                    Some(code)
                        if code == ERROR_UNABLE_TO_MOVE_REPLACEMENT as i32
                            || code == ERROR_UNABLE_TO_MOVE_REPLACEMENT_2 as i32
                ) =>
            {
                FailureClass::OutcomeUnknown
            }
            ReplaceOperation::ExistingTarget => FailureClass::NotCommitted,
            ReplaceOperation::NewTarget => FailureClass::OutcomeUnknown,
        }
    }

    fn replace_result(result: i32, operation: ReplaceOperation) -> Result<(), ReplaceFailure> {
        if result != 0 {
            return Ok(());
        }
        let source = io::Error::last_os_error();
        Err(match classify_failure(operation, source.raw_os_error()) {
            FailureClass::NotCommitted => ReplaceFailure::NotCommitted(source),
            FailureClass::OutcomeUnknown => ReplaceFailure::OutcomeUnknown(source),
        })
    }

    fn path_to_wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(iter::once(0))
            .collect()
    }

    fn win32_status(status: u32) -> io::Result<()> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }

    #[cfg(test)]
    fn bool_result(result: i32) -> io::Result<()> {
        if result != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Foundation::{
        ERROR_SHARING_VIOLATION, ERROR_UNABLE_TO_MOVE_REPLACEMENT,
        ERROR_UNABLE_TO_MOVE_REPLACEMENT_2,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, WRITE_DAC,
    };

    use super::{create_staged_file, existing_target, preserve_staged_for_recovery, windows};

    #[test]
    fn partial_replace_errors_are_outcome_unknown() {
        for raw_error in [
            ERROR_UNABLE_TO_MOVE_REPLACEMENT,
            ERROR_UNABLE_TO_MOVE_REPLACEMENT_2,
        ] {
            assert_eq!(
                windows::classify_failure(
                    windows::ReplaceOperation::ExistingTarget,
                    Some(raw_error as i32),
                ),
                windows::FailureClass::OutcomeUnknown
            );
        }
        assert_eq!(
            windows::classify_failure(windows::ReplaceOperation::ExistingTarget, Some(5)),
            windows::FailureClass::NotCommitted
        );
        assert_eq!(
            windows::classify_failure(windows::ReplaceOperation::NewTarget, Some(5)),
            windows::FailureClass::OutcomeUnknown
        );
    }

    #[test]
    fn unknown_replace_outcome_preserves_the_staged_recovery_file() {
        let directory = tempfile::tempdir().unwrap();
        let staged = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        std::fs::write(staged.path(), b"recoverable bytes").unwrap();
        let staged_path = staged.path().to_path_buf();

        let recovery_path = preserve_staged_for_recovery(staged.into_temp_path());

        assert_eq!(recovery_path.as_deref(), Some(staged_path.as_path()));
        assert_eq!(std::fs::read(&staged_path).unwrap(), b"recoverable bytes");
    }

    #[test]
    fn staged_overwrite_uses_target_dacl_and_blocks_shared_reads() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.json");
        std::fs::write(&target, b"old").unwrap();

        let target_dacl = windows::TargetDacl::read(&target).unwrap();
        let target_file = OpenOptions::new()
            .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
            .open(&target)
            .unwrap();
        target_dacl.apply_to(&target_file).unwrap();
        drop(target_file);

        let existing_target = existing_target(&target).unwrap().unwrap();
        let target_sddl = existing_target.dacl.sddl().unwrap();
        assert!(
            target_sddl.starts_with("D:P"),
            "the fixture requires a protected target DACL: {target_sddl}"
        );
        let mut staged = create_staged_file(
            directory.path(),
            ".jellyflow-stage-",
            Some(&existing_target),
        )
        .unwrap();

        assert_eq!(
            windows::TargetDacl::read(staged.path())
                .unwrap()
                .sddl()
                .unwrap(),
            target_sddl
        );
        assert_shared_read_is_blocked(staged.path());
        staged.as_file_mut().write_all(b"new secret").unwrap();
        assert_shared_read_is_blocked(staged.path());
    }

    fn assert_shared_read_is_blocked(path: &std::path::Path) {
        let error = OpenOptions::new()
            .read(true)
            .open(path)
            .expect_err("a second read handle must be blocked while staged content is live");
        assert_eq!(error.raw_os_error(), Some(ERROR_SHARING_VIOLATION as i32));
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use std::io;
    use std::path::Path;

    use super::{AtomicSaveError, AtomicSaveFault, existing_target, save_atomically};

    #[test]
    fn existing_target_open_is_nofollow_and_nonblocking() {
        let flags = super::existing_target_open_flags();
        assert_ne!(flags & libc::O_NOFOLLOW, 0);
        assert_ne!(flags & libc::O_NONBLOCK, 0);
    }

    #[test]
    fn overwrite_preserves_a_real_extended_acl() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.json");
        std::fs::write(&target, b"old").unwrap();

        match install_extended_acl(&target) {
            AclFixture::Installed => {}
            AclFixture::Unsupported(reason) => {
                eprintln!("SKIP: filesystem ACL fixture is unavailable: {reason}");
                return;
            }
        }
        let before = existing_target(&target).unwrap().unwrap();
        assert!(
            before.acl.has_extended_entries(),
            "the fixture must install an extended ACL"
        );

        let result: Result<(), AtomicSaveError<io::Error, io::Error>> = save_atomically(
            &target,
            ".jellyflow-acl-test-",
            AtomicSaveFault::None,
            || Ok(b"new".to_vec()),
            |_| Ok(()),
        );
        result.unwrap();

        let after = existing_target(&target).unwrap().unwrap();
        assert_eq!(after.acl, before.acl);
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            1,
            "a successful overwrite must not leave a staged file"
        );
    }

    enum AclFixture {
        Installed,
        Unsupported(String),
    }

    #[cfg(any(target_os = "android", target_os = "linux"))]
    fn install_extended_acl(path: &Path) -> AclFixture {
        use xattr::FileExt;

        const ACL_USER_OBJ: u16 = 0x01;
        const ACL_USER: u16 = 0x02;
        const ACL_GROUP_OBJ: u16 = 0x04;
        const ACL_MASK: u16 = 0x10;
        const ACL_OTHER: u16 = 0x20;
        const ACL_UNDEFINED_ID: u32 = u32::MAX;

        let mut acl = 2_u32.to_le_bytes().to_vec();
        push_acl_entry(&mut acl, ACL_USER_OBJ, 0o6, ACL_UNDEFINED_ID);
        push_acl_entry(&mut acl, ACL_USER, 0o4, 424_242);
        push_acl_entry(&mut acl, ACL_GROUP_OBJ, 0o4, ACL_UNDEFINED_ID);
        push_acl_entry(&mut acl, ACL_MASK, 0o4, ACL_UNDEFINED_ID);
        push_acl_entry(&mut acl, ACL_OTHER, 0, ACL_UNDEFINED_ID);

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        match file.set_xattr(std::ffi::OsStr::new("system.posix_acl_access"), &acl) {
            Ok(()) => AclFixture::Installed,
            Err(source) if acl_fixture_is_unavailable(&source) => {
                AclFixture::Unsupported(source.to_string())
            }
            Err(source) => panic!("install Linux ACL fixture: {source}"),
        }
    }

    #[cfg(any(target_os = "android", target_os = "linux"))]
    fn push_acl_entry(bytes: &mut Vec<u8>, tag: u16, permissions: u16, id: u32) {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&permissions.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }

    #[cfg(any(target_os = "android", target_os = "linux"))]
    fn acl_fixture_is_unavailable(error: &io::Error) -> bool {
        let Some(code) = error.raw_os_error() else {
            return false;
        };
        code == libc::ENOTSUP
            || code == libc::EOPNOTSUPP
            || code == libc::ENOSYS
            || code == libc::EPERM
            || code == libc::EACCES
    }

    #[cfg(target_os = "macos")]
    fn install_extended_acl(path: &Path) -> AclFixture {
        let output = std::process::Command::new("chmod")
            .arg("+a")
            .arg("everyone allow read")
            .arg(path)
            .output()
            .expect("run chmod for macOS ACL fixture");
        if output.status.success() {
            return AclFixture::Installed;
        }

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let unavailable = [
            "not supported",
            "Operation not permitted",
            "Permission denied",
        ]
        .iter()
        .any(|needle| stderr.contains(needle));
        if unavailable {
            AclFixture::Unsupported(stderr)
        } else {
            panic!("install macOS ACL fixture: {stderr}")
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "linux", target_os = "macos")))]
    fn install_extended_acl(_path: &Path) -> AclFixture {
        AclFixture::Unsupported("platform ACL adapter is unavailable".to_owned())
    }
}
