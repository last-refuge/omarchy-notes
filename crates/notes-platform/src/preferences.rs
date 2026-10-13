//! Versioned XDG preferences, atomically replaced by a bounded background writer.
use anyhow::{Context, Result, bail, ensure};
use notes_core::preferences::{Patch, Preferences, Save};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileFormat {
    version: u32,
    preferences: Preferences,
}

pub fn default_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| Path::new(s).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".config")))
        .context("Set HOME or XDG_CONFIG_HOME to remember preferences.")?;
    Ok(base.join("omarchy-notes-gpui/preferences.json"))
}
pub fn load(path: &Path) -> Result<Preferences> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    let file = match file {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => {
            return Err(e).context("Couldn’t read preferences. The existing file is unchanged.");
        }
    };
    ensure!(
        file.metadata()?.is_file(),
        "Preferences must be a regular file."
    );
    let mut bytes = vec![];
    file.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024,
        "The preferences file is too large. It was not changed."
    );
    let envelope: serde_json::Value = serde_json::from_slice(&bytes)
        .context("The preferences file is not valid JSON. It was not changed.")?;
    ensure!(
        envelope.get("version").and_then(|v| v.as_u64()) == Some(1),
        "This preferences version is not supported. The file was not changed."
    );
    let file: FileFormat = serde_json::from_value(envelope)?;
    file.preferences.validate()?;
    Ok(file.preferences)
}
pub fn save(path: &Path, patch: &Patch) -> Result<Preferences> {
    if patch.is_empty() {
        return load(path);
    }
    let parent = path
        .parent()
        .context("Preferences need a parent directory.")?;
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
        .custom_flags(libc::O_NOFOLLOW)
        .open(parent.join("preferences.lock"))?;
    ensure!(
        lock.metadata()?.is_file(),
        "Preferences lock must be a regular file."
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        // SAFETY: a live owned fd; flock is released when this File drops.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            break;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock {
            return Err(error.into());
        }
        if Instant::now() >= deadline {
            bail!("Another Notes window is saving preferences. Try again.");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut preferences = load(path)?;
    patch.apply(&mut preferences);
    preferences.validate()?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(
        &mut temp,
        &FileFormat {
            version: 1,
            preferences: preferences.clone(),
        },
    )?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    File::open(parent)?.sync_all()?;
    Ok(preferences)
}
pub struct Event {
    pub operation: u64,
    pub result: std::result::Result<Preferences, String>,
}
pub struct Worker {
    pub requests: mpsc::SyncSender<Save>,
    pub events: mpsc::Receiver<Event>,
}
impl Worker {
    /// Read before GUI startup. Errors leave a usable session with visible
    /// feedback; the writer refuses to overwrite invalid/future settings.
    pub fn start(path: PathBuf) -> Result<(Self, notes_core::preferences::Session)> {
        let (preferences, error) = match load(&path) {
            Ok(p) => (p, None),
            Err(e) => (Preferences::default(), Some(e.to_string())),
        };
        let (requests, jobs) = mpsc::sync_channel::<Save>(1);
        let (events, results) = mpsc::channel();
        thread::Builder::new()
            .name("notes-preferences".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = save(&path, &job.patch).map_err(|e| format!("{e:#}"));
                    if events
                        .send(Event {
                            operation: job.operation,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok((
            Self {
                requests,
                events: results,
            },
            notes_core::preferences::Session::new(preferences, error),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notes_core::preferences::{Appearance, NoteFont};
    #[test]
    fn defaults_are_read_only_and_saves_merge_and_reopen() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("config/preferences.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        assert!(!path.parent().unwrap().exists());
        save(
            &path,
            &Patch {
                appearance: Some(Appearance::Light),
                ..Default::default()
            },
        )
        .unwrap();
        save(
            &path,
            &Patch {
                note_font: Some(NoteFont::Sans),
                note_size: Some(25),
                ..Default::default()
            },
        )
        .unwrap();
        let saved = load(&path).unwrap();
        assert_eq!(saved.appearance, Appearance::Light);
        assert_eq!(saved.note_size, 25);
        assert_eq!(saved.note_font, NoteFont::Sans);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
    }
    #[test]
    fn corrupt_future_invalid_and_linked_files_are_preserved() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("preferences.json");
        let patch = Patch {
            note_size: Some(24),
            ..Default::default()
        };
        for bytes in [
            b"broken".to_vec(),
            br#"{"version":900,"preferences":{}}"#.to_vec(),
            br#"{"version":1,"preferences":{"note_size":0}}"#.to_vec(),
            vec![b' '; 65537],
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(load(&path).is_err());
            assert!(save(&path, &patch).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_file(&path).unwrap();
        let target = t.path().join("elsewhere");
        fs::write(&target, b"untouched").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(save(&path, &patch).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"untouched");
    }
    #[test]
    fn concurrent_owners_preserve_each_others_unrelated_fields() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("preferences.json");
        thread::scope(|s| {
            let path = &path;
            let a = s.spawn(move || {
                save(
                    path,
                    &Patch {
                        appearance: Some(Appearance::Light),
                        ..Default::default()
                    },
                )
                .unwrap()
            });
            let b = s.spawn(move || {
                save(
                    path,
                    &Patch {
                        list_width: Some(420),
                        ..Default::default()
                    },
                )
                .unwrap()
            });
            a.join().unwrap();
            b.join().unwrap();
        });
        let saved = load(&path).unwrap();
        assert_eq!(saved.appearance, Appearance::Light);
        assert_eq!(saved.list_width, 420);
    }
}
