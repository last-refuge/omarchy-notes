//! Linux QLockFile interoperability: exclusive creation, flock, and Qt owner data.
//! Protocol implementation; no Qt source is linked or copied.
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub struct LibraryLock {
    file: File,
    path: PathBuf,
}
impl LibraryLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(".lock");
        for _ in 0..3 {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(mut file) => {
                    file.try_lock()
                        .context("Could not lock the library; another app may be opening it.")?;
                    ensure!(
                        same_file(&file, &path),
                        "Library lock changed during startup. Try again."
                    );
                    let name = std::env::current_exe()?
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    let data = format!(
                        "{}\n{}\n{}\n{}\n{}\nomarchy-notes-rust-lock-v1\n",
                        std::process::id(),
                        name,
                        read("/proc/sys/kernel/hostname"),
                        read("/etc/machine-id"),
                        read("/proc/sys/kernel/random/boot_id")
                    );
                    let guard = Self {
                        file: file.try_clone()?,
                        path: path.clone(),
                    };
                    file.write_all(data.as_bytes())?;
                    file.sync_all()?;
                    return Ok(guard);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Reclaim only a recognisable dead Rust owner on this host. A Qt,
                    // malformed, remote or live owner is never removed by this edition.
                    let data = fs::read_to_string(&path).unwrap_or_default();
                    let lines: Vec<_> = data.lines().collect();
                    let dead = lines
                        .first()
                        .and_then(|s| s.parse::<u32>().ok())
                        .is_some_and(|pid| !Path::new(&format!("/proc/{pid}")).exists());
                    if lines.len() < 6
                        || lines[5] != "omarchy-notes-rust-lock-v1"
                        || lines[2] != read("/proc/sys/kernel/hostname")
                        || !dead
                    {
                        return Err(crate::LibraryBusy.into());
                    }
                    let old = OpenOptions::new().read(true).write(true).open(&path)?;
                    old.try_lock()
                        .context("Another Notes process owns this library.")?;
                    ensure!(
                        same_file(&old, &path),
                        "Library owner changed; try opening it again."
                    );
                    fs::remove_file(&path)?;
                }
                Err(error) => return Err(error).context("Could not create the library lock."),
            }
        }
        bail!("The library is being opened by another process.")
    }
}
fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().into()
}
fn same_file(file: &File, path: &Path) -> bool {
    match (file.metadata(), fs::metadata(path)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}
impl Drop for LibraryLock {
    fn drop(&mut self) {
        if same_file(&self.file, &self.path) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
