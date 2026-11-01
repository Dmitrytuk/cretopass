//! The atomic write layer.
//!
//! Every byte the gateway writes to disk — a vault, a plaintext export, a share
//! bundle, the app configuration — goes through this module. It guarantees that
//! a crash or a concurrent writer can never observe a half-written file: the
//! data is written to an exclusively created temp file beside the target,
//! `fsync`ed, then atomically `rename`d over the target, and the containing
//! directory is best-effort `fsync`ed so the rename itself survives a crash.
//!
//! The temp file is created through [`tempfile::NamedTempFile`], which opens it
//! `O_CREAT | O_EXCL` under a randomly generated name at mode `0600` on Unix, so
//! a pre-planted symlink at the temp path is never followed and two concurrent
//! writers never collide. Dropping an [`AtomicFile`] that was never committed
//! removes the temp file.
//!
//! # Durability per platform
//!
//! - **Linux / macOS.** The file contents are `fsync`ed before the rename, and
//!   the parent directory is `fsync`ed after it, so both the content and the
//!   rename survive a crash. [`AtomicFile::commit_new`] uses a no-clobber
//!   rename (`renameat2(RENAME_NOREPLACE)` / `renamex_np`), falling back to
//!   `link` + `unlink` where the kernel or filesystem lacks it (a crash between
//!   the two can leave a temporary hard link behind, but never a clobbered
//!   target).
//! - **Windows.** The rename is atomic but a directory cannot be `fsync`ed with
//!   `std`, so the rename's durability is best-effort only.
//! - **Other platforms.** Directory `fsync` is skipped when the platform
//!   refuses to open a directory; that limitation is silent and best-effort.
//!
//! In every case, the *content* of the target is never a torn write: a reader
//! sees either the previous file or the complete new one.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::{Builder, NamedTempFile};

use super::cancel::CancelToken;

/// Mode (Unix) a secret-bearing file is created with: owner only.
pub(crate) const SECRET_FILE_MODE: u32 = 0o600;

/// A failure this layer can report when writing.
///
/// The atomic layer translates the raw OS errors it can act on into domain
/// outcomes, so no caller outside this module has to match an
/// [`io::ErrorKind`]. It composes into [`FileHandlingError`](super::FileHandlingError)
/// through `From`.
#[derive(Debug)]
pub(crate) enum WriteError {
    /// The OS refused the write and this layer could not classify it.
    Io(io::Error),
    /// A no-clobber commit found the target already occupied.
    AlreadyExists(PathBuf),
    /// A temp file could not be created beside the target, so the target's
    /// directory is not writable (missing directory, no permission, read-only
    /// filesystem, or full disk).
    NotWritable(PathBuf),
    /// The owner cancelled the write before it committed.
    Abandoned,
}

/// A temporary file created exclusively beside its target.
///
/// Create it with [`AtomicFile::new`], write with
/// [`write_all_and_sync`](AtomicFile::write_all_and_sync), then either
/// [`commit`](AtomicFile::commit) (replace the target) or
/// [`commit_new`](AtomicFile::commit_new) (refuse an existing target). Dropping
/// an uncommitted value removes the temp file, so a failed write leaves nothing
/// behind.
pub(crate) struct AtomicFile {
    temp: NamedTempFile,
    target: PathBuf,
}

impl AtomicFile {
    /// Creates a temp file for `target` in `target`'s own directory, with
    /// `mode` (Unix only) and a random suffix.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::NotWritable`] when no temp file can be created
    /// (for example when the directory does not exist or is not writable), and
    /// [`WriteError::Io`] when the temp file is created but its mode cannot be
    /// set.
    pub(crate) fn new(target: &Path, mode: u32) -> Result<Self, WriteError> {
        let dir = parent_dir(target);
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("vault");
        let temp = Builder::new()
            .prefix(&format!(".{name}."))
            .suffix(".tmp")
            .tempfile_in(dir)
            .map_err(|_| WriteError::NotWritable(target.to_path_buf()))?;

        let this = Self {
            temp,
            target: target.to_path_buf(),
        };
        this.set_mode(mode)?;
        Ok(this)
    }

    /// Writes `data` and `fsync`s the temp file.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Io`] when the write or `fsync` fails.
    pub(crate) fn write_all_and_sync(&mut self, data: &[u8]) -> Result<(), WriteError> {
        self.temp.write_all(data).map_err(WriteError::Io)?;
        self.temp.as_file().sync_all().map_err(WriteError::Io)
    }

    /// Sets the temp file's mode (Unix only; a no-op elsewhere).
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Io`] when the mode cannot be set.
    pub(crate) fn set_mode(&self, mode: u32) -> Result<(), WriteError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            self.temp
                .as_file()
                .set_permissions(fs::Permissions::from_mode(mode))
                .map_err(WriteError::Io)?;
        }
        // `mode` is only meaningful on Unix; this keeps it "used" elsewhere.
        let _ = mode;
        Ok(())
    }

    /// Atomically renames the temp file over `target`, replacing any existing
    /// file, then best-effort `fsync`s the containing directory.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Io`] when the rename fails. The temp file is
    /// removed in either case.
    pub(crate) fn commit(self) -> Result<(), WriteError> {
        let target = self.target;
        // `PersistError` returns the temp file, which dropping removes.
        self.temp
            .persist(&target)
            .map_err(|error| WriteError::Io(error.into()))?;
        sync_parent_dir(&target);
        Ok(())
    }

    /// Atomically renames the temp file to `target` only if `target` does not
    /// already exist, then best-effort `fsync`s the containing directory.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::AlreadyExists`] when `target` exists, and
    /// [`WriteError::Io`] when the rename fails for any other reason. The temp
    /// file is removed in either case.
    pub(crate) fn commit_new(self) -> Result<(), WriteError> {
        let target = self.target;
        // The no-clobber rename is the only place this layer must interpret an
        // `io::ErrorKind`: `persist_noclobber` reports an occupied target as
        // `AlreadyExists`. Everything above this point is `Io`.
        self.temp
            .persist_noclobber(&target)
            .map_err(|error| match error.error.kind() {
                io::ErrorKind::AlreadyExists => WriteError::AlreadyExists(target.clone()),
                _ => WriteError::Io(error.error),
            })?;
        sync_parent_dir(&target);
        Ok(())
    }
}

/// Writes `data` to `target` atomically, preserving the mode of an existing
/// regular target.
///
/// # Errors
///
/// Returns [`WriteError::NotWritable`] when the temp file cannot be created,
/// and [`WriteError::Io`] when it cannot be written, `fsync`ed or renamed.
pub(crate) fn write_atomic(target: &Path, data: &[u8]) -> Result<(), WriteError> {
    write_atomic_cancellable(target, data, None)
}

/// Writes `data` to `target` atomically unless `cancel` is set.
///
/// The cancellation is checked after the temp file is written and `fsync`ed but
/// immediately before the rename, so an abandoned writer (a force-quit whose
/// save has stalled) discards its temp file instead of replacing whatever is at
/// `target` now. Nothing is renamed away before that check.
///
/// # Errors
///
/// Returns [`WriteError::Abandoned`] when `cancel` is set by the time the check
/// runs, [`WriteError::NotWritable`] when the temp file cannot be created, and
/// [`WriteError::Io`] when it cannot be written, `fsync`ed or renamed.
pub(crate) fn write_atomic_cancellable(
    target: &Path,
    data: &[u8],
    cancel: Option<&CancelToken>,
) -> Result<(), WriteError> {
    let mut temp = AtomicFile::new(target, SECRET_FILE_MODE)?;
    preserve_target_mode(target, &temp)?;
    temp.write_all_and_sync(data)?;

    if cancel.is_some_and(CancelToken::is_cancelled) {
        // Dropping `temp` removes it, so the target is left exactly as it was.
        return Err(WriteError::Abandoned);
    }

    temp.commit()
}

/// Writes plaintext `data` to `target` atomically at owner-only permissions,
/// never inheriting a more permissive mode from an existing file — the output
/// holds decrypted secrets (an export) and must not be group/world readable.
///
/// # Errors
///
/// Returns [`WriteError::NotWritable`] when the temp file cannot be created,
/// and [`WriteError::Io`] when it cannot be written, `fsync`ed or renamed.
pub(crate) fn write_atomic_private(target: &Path, data: &[u8]) -> Result<(), WriteError> {
    let mut temp = AtomicFile::new(target, SECRET_FILE_MODE)?;
    temp.write_all_and_sync(data)?;
    temp.commit()
}

/// Creates `target` atomically and refuses to replace an existing file (or a
/// symlink) with a single no-clobber rename.
///
/// # Errors
///
/// Returns [`WriteError::AlreadyExists`] when `target` exists,
/// [`WriteError::NotWritable`] when the target's directory is missing, and any
/// other [`WriteError::Io`] when the write fails.
pub(crate) fn write_atomic_new(target: &Path, data: &[u8]) -> Result<(), WriteError> {
    let mut temp = AtomicFile::new(target, SECRET_FILE_MODE)?;
    temp.write_all_and_sync(data)?;
    temp.commit_new()
}

/// Checks whether `target`'s directory is writable by creating and removing an
/// exclusive temp file beside it (never following a planted symlink).
///
/// # Errors
///
/// Returns [`WriteError::NotWritable`] when no temp file can be created.
pub(crate) fn probe_writable(target: &Path) -> Result<(), WriteError> {
    AtomicFile::new(target, SECRET_FILE_MODE).map(|_| ())
}

/// Opens `path` for reading, refusing to follow a symlink at `path`.
///
/// A vault reached *through* a symlink is therefore rejected rather than read.
/// That is deliberate — a planted link must never redirect a read of a secret —
/// and surfaces as an [`io::Error`] (`ELOOP` on Unix).
///
/// # Errors
///
/// Returns an [`io::Error`] when `path` cannot be opened, is a symlink (Unix),
/// or is not a regular file.
pub(crate) fn open_for_read(path: &Path) -> io::Result<File> {
    let file = open_nofollow(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to read a non-regular file",
        ));
    }
    Ok(file)
}

/// Opens `path` without following a symlink where the platform exposes a flag.
fn open_nofollow(path: &Path) -> io::Result<File> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::fs::OpenOptionsExt;

        // Hard-coded `O_NOFOLLOW`: the `libc` crate is not a dependency and the
        // gateway forbids `unsafe`, so the constant cannot be imported.
        #[cfg(target_os = "linux")]
        const O_NOFOLLOW: i32 = 0o400000;
        #[cfg(target_os = "macos")]
        const O_NOFOLLOW: i32 = 0x100;

        fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NOFOLLOW)
            .open(path)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        // No portable flag: refuse an obvious symlink before opening. This is
        // not atomic (a TOCTOU window remains) and is documented as best-effort.
        if fs::symlink_metadata(path)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to open a symlink",
            ));
        }
        File::open(path)
    }
}

/// The directory holding `path`, or `.` for a bare file name.
fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Best-effort `fsync` of `path`'s parent directory so a rename into it is
/// durable after a crash. Not every platform allows opening a directory, so a
/// failure is ignored.
fn sync_parent_dir(path: &Path) {
    if let Ok(handle) = File::open(parent_dir(path)) {
        let _ = handle.sync_all();
    }
}

/// Copies `target`'s permission bits onto a freshly created temp file, so
/// replacing an existing regular file preserves the mode the user gave it.
///
/// Uses `symlink_metadata`: a mode is never inherited from a symlink's target,
/// only from an ordinary existing file.
fn preserve_target_mode(target: &Path, temp: &AtomicFile) -> Result<(), WriteError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::symlink_metadata(target)
            && meta.is_file()
        {
            let mode = meta.permissions().mode();
            temp.set_mode(mode)?;
        }
    }
    #[cfg(not(unix))]
    let _ = (target, temp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_write_leaves_the_target_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");
        fs::write(&target, b"older contents").unwrap();

        let cancel = CancelToken::new();
        cancel.cancel();

        let error =
            write_atomic_cancellable(&target, b"newer contents", Some(&cancel)).unwrap_err();

        assert!(matches!(error, WriteError::Abandoned));
        assert_eq!(fs::read(&target).unwrap(), b"older contents");
        // The abandoned temp file is removed, not left behind. On the vault
        // path it holds ciphertext; on an export it would be plaintext.
        let left: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "only the target must remain");
    }

    #[test]
    fn an_uncancelled_write_commits() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");
        let cancel = CancelToken::new();

        write_atomic_cancellable(&target, b"contents", Some(&cancel)).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"contents");
        assert!(!cancel.is_cancelled());
    }

    #[test]
    fn write_atomic_replaces_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"second");
        assert_no_temp_left(dir.path());
    }

    #[test]
    fn write_atomic_creates_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        write_atomic(&target, b"data").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"data");
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_preserves_the_existing_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");
        write_atomic(&target, b"first").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();

        write_atomic(&target, b"second").unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640, "a replacement keeps the file's mode");
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_private_never_inherits_a_permissive_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("export.json");
        write_atomic(&target, b"first").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();

        write_atomic_private(&target, b"second").unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, SECRET_FILE_MODE, "plaintext must stay owner-only");
    }

    #[test]
    fn write_atomic_new_creates_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        write_atomic_new(&target, b"data").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"data");
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_new_creates_an_owner_only_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        write_atomic_new(&target, b"data").unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, SECRET_FILE_MODE,
            "a created secret must be owner-only"
        );
    }

    #[test]
    fn write_atomic_new_refuses_an_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");
        fs::write(&target, b"keep me").unwrap();

        let result = write_atomic_new(&target, b"clobber");

        assert!(
            matches!(result, Err(WriteError::AlreadyExists(_))),
            "a no-clobber write must refuse an existing file: {result:?}"
        );
        assert_eq!(fs::read(&target).unwrap(), b"keep me");
        assert_no_temp_left(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_new_refuses_a_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, b"do not touch").unwrap();
        let link = dir.path().join("vault.age");
        symlink(&victim, &link).unwrap();

        let result = write_atomic_new(&link, b"clobber");

        assert!(result.is_err(), "a symlink is not a free name");
        assert_eq!(fs::read(&victim).unwrap(), b"do not touch");
    }

    #[test]
    fn an_uncommitted_temp_is_removed_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        let temp = AtomicFile::new(&target, SECRET_FILE_MODE).unwrap();
        let temp_path = temp.temp.path().to_path_buf();
        assert!(temp_path.exists());

        drop(temp);

        assert!(
            !temp_path.exists(),
            "an uncommitted temp must be cleaned up"
        );
        assert!(!target.exists(), "the target must not have been created");
    }

    #[test]
    fn two_temp_files_for_one_target_do_not_collide() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        let a = AtomicFile::new(&target, SECRET_FILE_MODE).unwrap();
        let b = AtomicFile::new(&target, SECRET_FILE_MODE).unwrap();

        assert_ne!(a.temp.path(), b.temp.path());
    }

    #[test]
    fn a_failed_commit_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        // A directory at the target path makes the rename fail.
        let target = dir.path().join("vault.age");
        fs::create_dir(&target).unwrap();

        let mut temp = AtomicFile::new(&target, SECRET_FILE_MODE).unwrap();
        temp.write_all_and_sync(b"data").unwrap();
        let result = temp.commit();

        assert!(result.is_err());
        assert!(target.is_dir(), "the directory is untouched");
        assert_no_temp_left(dir.path());
    }

    #[test]
    fn probe_writable_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        probe_writable(&target).unwrap();

        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "the probe must not leave its temp file behind"
        );
    }

    #[test]
    fn probe_writable_rejects_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing").join("vault.age");

        assert!(probe_writable(&missing).is_err());
    }

    #[test]
    fn open_for_read_reads_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        fs::write(&path, b"contents").unwrap();

        let mut file = open_for_read(&path).unwrap();
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf).unwrap();

        assert_eq!(buf, b"contents");
    }

    #[cfg(unix)]
    #[test]
    fn open_for_read_refuses_a_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, b"do not follow").unwrap();
        let link = dir.path().join("vault.age");
        symlink(&victim, &link).unwrap();

        assert!(
            open_for_read(&link).is_err(),
            "a symlink must not be opened"
        );
    }

    #[test]
    fn open_for_read_refuses_a_directory() {
        let dir = tempfile::tempdir().unwrap();

        assert!(open_for_read(dir.path()).is_err());
    }

    /// Asserts the directory holds no leftover `.tmp` temp files.
    fn assert_no_temp_left(dir: &Path) {
        let leftovers: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temp file may survive: {leftovers:?}"
        );
    }
}
