//! Per-library presentation state, separate from global preferences and note data.
//!
//! Discover identity before GUI startup. Loading, parsing, locking, serialization
//! and durable writes happen on the worker, never in a render callback. One
//! pending snapshot replaces older pending work; receipts identify only the
//! operation actually committed, so callers keep newer local edits unchanged.
use anyhow::{Context, Result, bail, ensure};
use notes_core::library::Sort;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{ffi::OsStrExt, fs::DirBuilderExt, fs::OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

/// These limits are independent of the 64 KiB global preferences envelope.
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_COLLAPSED_FOLDERS: usize = 32_768;
pub const MAX_FOLDER_ID_BYTES: usize = 4 * 1024;

/// Exact canonical Unix path identity; path aliases share a key and libraries
/// with the same basename do not. The envelope verifies this key on every read.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibraryKey(String);
impl LibraryKey {
    /// Filesystem work: call at startup or on a background executor, not render.
    pub fn discover(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .context("Couldn’t identify this library for folder view settings.")?;
        ensure!(root.is_dir(), "The library root must be a directory.");
        Ok(Self(format!(
            "{:x}",
            Sha256::digest(root.as_os_str().as_bytes())
        )))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// `config_root` is the XDG configuration base, not the application prefix.
    pub fn path_in(&self, config_root: &Path) -> PathBuf {
        config_root
            .join("omarchy-notes-gpui/library-views")
            .join(format!("{}.json", self.0))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewState {
    pub collapsed_folders: BTreeSet<String>,
    pub sort: Sort,
}
impl ViewState {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.collapsed_folders.len() <= MAX_COLLAPSED_FOLDERS,
            "Too many collapsed folders to remember."
        );
        let mut allocated = 0usize;
        for id in &self.collapsed_folders {
            ensure!(
                !id.is_empty() && id.len() <= MAX_FOLDER_ID_BYTES,
                "A collapsed folder identity is empty or too long."
            );
            // Bound retained job/receipt strings even when a caller supplies a
            // tiny ID backed by an unusually large allocation. Tree entries are
            // separately bounded by MAX_COLLAPSED_FOLDERS.
            allocated = allocated.saturating_add(id.capacity());
            ensure!(
                allocated <= MAX_FILE_BYTES,
                "Collapsed folder identities exceed the view-state memory limit."
            );
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileFormat {
    version: u32,
    library: String,
    view: ViewState,
}
#[derive(Serialize)]
struct FileRef<'a> {
    version: u32,
    library: &'a str,
    view: &'a ViewState,
}

pub fn default_path(library: &LibraryKey) -> Result<PathBuf> {
    let preferences = crate::preferences::default_path()?;
    let prefix = preferences
        .parent()
        .context("Folder view settings need a configuration directory.")?;
    Ok(prefix
        .join("library-views")
        .join(format!("{}.json", library.0)))
}

/// Missing state is read-only and returns defaults. Invalid/future state is an
/// error, not permission to replace the file with defaults.
pub fn load(path: &Path, library: &LibraryKey) -> Result<ViewState> {
    load_with_default(path, library, ViewState::default())
}
fn load_with_default(path: &Path, library: &LibraryKey, fallback: ViewState) -> Result<ViewState> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(fallback);
        }
        Err(error) => return Err(error).context("Couldn’t read folder view settings."),
    };
    ensure!(
        file.metadata()?.is_file(),
        "Folder view settings must be a regular file."
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_FILE_BYTES,
        "Folder view settings are too large. The file was not changed."
    );
    let envelope: serde_json::Value = serde_json::from_slice(&bytes)
        .context("Folder view settings are not valid JSON. The file was not changed.")?;
    ensure!(
        envelope.get("version").and_then(|v| v.as_u64()) == Some(1),
        "This folder view settings version is not supported. The file was not changed."
    );
    // Parse the original bytes so duplicate named fields are rejected, too.
    let file: FileFormat = serde_json::from_slice(&bytes)?;
    ensure!(
        file.library == library.0,
        "Folder view settings belong to another library. The file was not changed."
    );
    file.view.validate()?;
    Ok(file.view)
}

/// Full snapshots have one owning library session. The UI prunes vanished IDs
/// and supplies its newest sort/collapse state together, rather than merging a
/// stale disk snapshot into a newer local view.
pub fn save(path: &Path, library: &LibraryKey, view: &ViewState) -> Result<()> {
    view.validate()?;
    // Limit the encoded envelope, including escaping/format overhead, without
    // allowing serialization to grow its encoded payload beyond the file budget.
    let mut bytes = LimitedBytes(Vec::new());
    serde_json::to_writer(
        &mut bytes,
        &FileRef {
            version: 1,
            library: library.as_str(),
            view,
        },
    )?;
    bytes.write_all(b"\n")?;
    let parent = path
        .parent()
        .context("Folder view settings need a parent directory.")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path.with_extension("lock"))?;
    ensure!(
        lock.metadata()?.is_file(),
        "Folder view settings lock must be a regular file."
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        // SAFETY: a live owned fd. Closing the File releases the advisory lock.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            break;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock {
            return Err(error.into());
        }
        if Instant::now() >= deadline {
            bail!("Another Notes window is saving folder view settings. Try again.");
        }
        thread::sleep(Duration::from_millis(10));
    }
    // Refuse corrupt, unsupported, symlinked, or mismatched existing state,
    // including changes another owner made while this save waited for the lock.
    load(path, library)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes.0)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

struct LimitedBytes(Vec<u8>);
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_FILE_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other(
                "Folder view settings exceed the file size limit.",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct Save {
    pub operation: u64,
    pub state: ViewState,
}
#[derive(Debug)]
pub enum Event {
    Loaded {
        library: LibraryKey,
        result: std::result::Result<ViewState, String>,
    },
    Saved {
        library: LibraryKey,
        operation: u64,
        result: std::result::Result<ViewState, String>,
    },
}
#[derive(Default)]
struct Mailbox {
    pending: Option<Save>,
    last_operation: Option<u64>,
    closed: bool,
}
type SharedMailbox = Arc<(Mutex<Mailbox>, Condvar)>;

pub struct Worker {
    library: LibraryKey,
    mailbox: SharedMailbox,
    pub events: mpsc::Receiver<Event>,
}
impl Worker {
    /// Returns before the initial read. At most two receipts, one running save,
    /// and one pending save are retained; slow polling cannot grow a queue.
    pub fn start(library: LibraryKey) -> Result<Self> {
        Self::start_with_default(library, ViewState::default())
    }
    /// Legacy presentation choices apply only when this library has no view
    /// file. Existing state, including explicitly saved Edited sort, wins.
    /// Loading never creates a file: the caller can submit the loaded/current
    /// state once to establish the migration through normal save receipts.
    pub fn start_with_default(library: LibraryKey, fallback: ViewState) -> Result<Self> {
        let path = default_path(&library)?;
        Self::at_path(library, path, fallback)
    }
    /// Explicit XDG configuration base for embedding and isolated tests.
    pub fn start_in(library: LibraryKey, config_root: &Path) -> Result<Self> {
        Self::start_in_with_default(library, config_root, ViewState::default())
    }
    pub fn start_in_with_default(
        library: LibraryKey,
        config_root: &Path,
        fallback: ViewState,
    ) -> Result<Self> {
        let path = library.path_in(config_root);
        Self::at_path(library, path, fallback)
    }
    fn at_path(library: LibraryKey, path: PathBuf, fallback: ViewState) -> Result<Self> {
        fallback.validate()?;
        let mailbox: SharedMailbox = Arc::default();
        let jobs = mailbox.clone();
        let identity = library.clone();
        let (events, results) = mpsc::sync_channel(2);
        thread::Builder::new()
            .name("notes-library-view".into())
            .spawn(move || {
                if events
                    .send(Event::Loaded {
                        library: identity.clone(),
                        result: load_with_default(&path, &identity, fallback)
                            .map_err(|e| format!("{e:#}")),
                    })
                    .is_err()
                {
                    return;
                }
                loop {
                    let (mutex, wake) = &*jobs;
                    let mut inbox = mutex.lock().unwrap_or_else(|e| e.into_inner());
                    while inbox.pending.is_none() && !inbox.closed {
                        inbox = wake.wait(inbox).unwrap_or_else(|e| e.into_inner());
                    }
                    // Drop cancels pending work; an already running atomic save
                    // finishes without depending on the GUI consuming a receipt.
                    if inbox.closed {
                        break;
                    }
                    let job = inbox.pending.take().expect("pending save was checked");
                    drop(inbox);
                    let result = save(&path, &identity, &job.state)
                        .map(|()| job.state)
                        .map_err(|e| format!("{e:#}"));
                    if events
                        .send(Event::Saved {
                            library: identity.clone(),
                            operation: job.operation,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            library,
            mailbox,
            events: results,
        })
    }
    pub fn library(&self) -> &LibraryKey {
        &self.library
    }
    /// No disk access or serialization. Validation is bounded by the ID budget.
    /// A coalesced operation has no receipt; only its replacement can commit.
    /// Operation numbers must increase within this worker's library generation.
    pub fn submit(&self, job: Save) -> std::result::Result<(), String> {
        job.state.validate().map_err(|e| e.to_string())?;
        let (mutex, wake) = &*self.mailbox;
        let mut inbox = mutex.lock().unwrap_or_else(|e| e.into_inner());
        if inbox.closed {
            return Err("Folder view settings writer is closed.".into());
        }
        if inbox
            .last_operation
            .is_some_and(|last| job.operation <= last)
        {
            return Err("Folder view settings operation is stale.".into());
        }
        inbox.last_operation = Some(job.operation);
        let old = inbox.pending.replace(job);
        wake.notify_one();
        drop(inbox);
        drop(old);
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let (mutex, wake) = &*self.mailbox;
        let mut inbox = mutex.lock().unwrap_or_else(|e| e.into_inner());
        inbox.closed = true;
        let pending = inbox.pending.take();
        wake.notify_one();
        drop(inbox);
        drop(pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn fixture() -> (tempfile::TempDir, LibraryKey, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("library");
        fs::create_dir(&root).unwrap();
        let key = LibraryKey::discover(&root).unwrap();
        let path = key.path_in(&temp.path().join("config"));
        (temp, key, path)
    }
    fn view(folder: &str, sort: Sort) -> ViewState {
        ViewState {
            collapsed_folders: [folder.to_owned()].into(),
            sort,
        }
    }
    #[test]
    fn canonical_aliases_share_identity_but_same_names_and_raw_paths_do_not() {
        let (temp, key, _) = fixture();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(temp.path().join("library"), &alias).unwrap();
        assert_eq!(key, LibraryKey::discover(&alias).unwrap());
        let other = temp.path().join("another/library");
        fs::create_dir_all(&other).unwrap();
        assert_ne!(key, LibraryKey::discover(&other).unwrap());
        for byte in [0xfe, 0xff] {
            let root = temp
                .path()
                .join(std::ffi::OsString::from_vec(vec![b'l', byte]));
            fs::create_dir(&root).unwrap();
        }
        assert_ne!(
            LibraryKey::discover(
                &temp
                    .path()
                    .join(std::ffi::OsString::from_vec(vec![b'l', 0xfe]))
            )
            .unwrap(),
            LibraryKey::discover(
                &temp
                    .path()
                    .join(std::ffi::OsString::from_vec(vec![b'l', 0xff]))
            )
            .unwrap()
        );
    }
    #[test]
    fn defaults_are_read_only_and_large_per_library_state_is_independent() {
        let (temp, key, path) = fixture();
        assert_eq!(load(&path, &key).unwrap(), ViewState::default());
        assert!(!path.parent().unwrap().exists());
        let global = temp
            .path()
            .join("config/omarchy-notes-gpui/preferences.json");
        fs::create_dir_all(global.parent().unwrap()).unwrap();
        fs::write(&global, b"global settings untouched").unwrap();
        let state = ViewState {
            collapsed_folders: (0..10_000)
                .map(|i| format!("legacy-folder-identity-{i:05}"))
                .collect(),
            sort: Sort::Title,
        };
        save(&path, &key, &state).unwrap();
        assert!(fs::metadata(&path).unwrap().len() > 64 * 1024);
        assert_eq!(load(&path, &key).unwrap(), state);
        assert_eq!(fs::read(&global).unwrap(), b"global settings untouched");
        let second_root = temp.path().join("second-library");
        fs::create_dir(&second_root).unwrap();
        let second = LibraryKey::discover(&second_root).unwrap();
        let second_path = second.path_in(&temp.path().join("config"));
        save(&second_path, &second, &view("other", Sort::Created)).unwrap();
        assert_eq!(load(&path, &key).unwrap(), state);
        assert_eq!(load(&second_path, &second).unwrap().sort, Sort::Created);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 4);
    }
    #[test]
    fn corrupt_future_mismatched_and_linked_files_are_preserved() {
        let (temp, key, path) = fixture();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for bytes in [
            b"broken".to_vec(),
            br#"{"version":900,"view":{}}"#.to_vec(),
            format!(
                r#"{{"version":1,"library":"{}","view":{{"collapsed_folders":[],"sort":"unknown"}}}}"#,
                key.0
            )
            .into_bytes(),
            br#"{"version":1,"library":"another","view":{"collapsed_folders":[],"sort":"edited"}}"#
                .to_vec(),
            vec![b' '; MAX_FILE_BYTES + 1],
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(load(&path, &key).is_err());
            assert!(save(&path, &key, &view("safe", Sort::Edited)).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_file(&path).unwrap();
        let target = temp.path().join("untouched");
        fs::write(&target, b"original").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(load(&path, &key).is_err());
        assert!(save(&path, &key, &ViewState::default()).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }
    #[test]
    fn validation_and_encoded_size_fail_before_replacing_a_valid_file() {
        let (_temp, key, path) = fixture();
        let original = view("original", Sort::Edited);
        save(&path, &key, &original).unwrap();
        for invalid in [
            view("", Sort::Edited),
            view(&"a".repeat(MAX_FOLDER_ID_BYTES + 1), Sort::Edited),
            ViewState {
                collapsed_folders: (0..=MAX_COLLAPSED_FOLDERS).map(|i| i.to_string()).collect(),
                sort: Sort::Edited,
            },
        ] {
            assert!(save(&path, &key, &invalid).is_err());
            assert_eq!(load(&path, &key).unwrap(), original);
        }
        // IDs are small enough individually, but JSON control-character escapes
        // exceed the independent encoded envelope budget.
        let escaped = ViewState {
            collapsed_folders: (0..2000)
                .map(|i| format!("{i}\u{0001}{}", "\u{0001}".repeat(1000)))
                .collect(),
            sort: Sort::Created,
        };
        assert!(escaped.validate().is_ok());
        assert!(save(&path, &key, &escaped).is_err());
        assert_eq!(load(&path, &key).unwrap(), original);
        let mut oversized = String::with_capacity(MAX_FILE_BYTES + 1);
        oversized.push('x');
        assert!(
            ViewState {
                collapsed_folders: [oversized].into(),
                sort: Sort::Edited,
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn worker_loads_async_and_preserves_failed_load_on_save() {
        let (temp, key, path) = fixture();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let future = br#"{"version":900,"view":{}}"#;
        fs::write(&path, future).unwrap();
        let worker = Worker::start_in(key.clone(), &temp.path().join("config")).unwrap();
        match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            Event::Loaded { library, result } => {
                assert_eq!(library, key);
                assert!(result.is_err());
            }
            Event::Saved { .. } => panic!("load must precede any save receipt"),
        }
        worker
            .submit(Save {
                operation: 1,
                state: view("changed", Sort::Title),
            })
            .unwrap();
        match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            Event::Saved {
                library,
                operation,
                result,
            } => {
                assert_eq!(library, key);
                assert_eq!(operation, 1);
                assert!(result.is_err());
            }
            Event::Loaded { .. } => panic!("only one initial load"),
        }
        assert_eq!(fs::read(&path).unwrap(), future);
    }
    #[test]
    fn legacy_sort_fallback_applies_only_to_missing_per_library_state() {
        let (temp, key, path) = fixture();
        let config = temp.path().join("config");
        let title = ViewState {
            sort: Sort::Title,
            ..Default::default()
        };
        let worker = Worker::start_in_with_default(key.clone(), &config, title.clone()).unwrap();
        match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            Event::Loaded { result, .. } => assert_eq!(result.unwrap(), title),
            Event::Saved { .. } => panic!("initial load is not a save"),
        }
        assert!(!path.exists());
        // The caller establishes the migration once with a normal durable save.
        worker
            .submit(Save {
                operation: 1,
                state: title,
            })
            .unwrap();
        match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            Event::Saved { result, .. } => assert_eq!(result.unwrap().sort, Sort::Title),
            Event::Loaded { .. } => panic!("load receipt repeated"),
        }
        drop(worker);
        let edited = view("persisted-folder", Sort::Edited);
        save(&path, &key, &edited).unwrap();
        let worker = Worker::start_in_with_default(
            key.clone(),
            &config,
            ViewState {
                sort: Sort::Title,
                ..Default::default()
            },
        )
        .unwrap();
        match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
            Event::Loaded { result, .. } => assert_eq!(result.unwrap(), edited),
            Event::Saved { .. } => panic!("existing state must only be loaded"),
        }
        assert_eq!(load(&path, &key).unwrap(), edited);
    }
    #[test]
    fn slow_polling_coalesces_pending_jobs_and_receipts_prove_exact_operations() {
        let (temp, key, path) = fixture();
        let worker = Worker::start_in(key.clone(), &temp.path().join("config")).unwrap();
        worker
            .submit(Save {
                operation: 1,
                state: view("one", Sort::Edited),
            })
            .unwrap();
        // Initial load + receipt 1 fill the bounded receipt channel. Once file 2
        // is committed its receipt blocks, allowing deterministic coalescing.
        let deadline = Instant::now() + Duration::from_secs(2);
        while load(&path, &key).unwrap().collapsed_folders != BTreeSet::from(["one".to_owned()]) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        worker
            .submit(Save {
                operation: 2,
                state: view("two", Sort::Created),
            })
            .unwrap();
        while load(&path, &key).unwrap().sort != Sort::Created {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        for operation in [3, 4] {
            worker
                .submit(Save {
                    operation,
                    state: view("newest", Sort::Title),
                })
                .unwrap();
        }
        assert!(
            worker
                .submit(Save {
                    operation: 4,
                    state: ViewState::default(),
                })
                .is_err()
        );
        let mut operations = Vec::new();
        while operations.last() != Some(&4) {
            match worker.events.recv_timeout(Duration::from_secs(2)).unwrap() {
                Event::Loaded { library, result } => {
                    assert_eq!(library, key);
                    assert!(result.is_ok());
                }
                Event::Saved {
                    library,
                    operation,
                    result,
                } => {
                    assert_eq!(library, key);
                    let saved = result.unwrap();
                    assert_eq!(
                        saved.sort,
                        match operation {
                            1 => Sort::Edited,
                            2 => Sort::Created,
                            4 => Sort::Title,
                            _ => panic!("superseded job produced a receipt"),
                        }
                    );
                    operations.push(operation);
                }
            }
        }
        assert_eq!(operations, [1, 2, 4]);
        assert_eq!(load(&path, &key).unwrap(), view("newest", Sort::Title));
    }
}
