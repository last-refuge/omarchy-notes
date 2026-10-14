mod capture_command;
mod launch_command;
use anyhow::{Context, Result, bail, ensure};
use std::path::PathBuf;

fn main() {
    if let Err(error) = start() {
        eprintln!("Omarchy Notes: {error:#}");
        std::process::exit(1);
    }
}
fn start() -> Result<()> {
    let mut appearance = None;
    let mut directory = None;
    let mut demo = false;
    let mut quick = false;
    let mut new = false;
    let mut note = None;
    let mut capture = None;
    let mut request_id = None;
    let mut capture_status = None;
    let mut backup = None;
    let mut inspect = None;
    let mut restore = None;
    let mut fail = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!(
                    "{}",
                    "Omarchy Notes\n\nUsage: omarchy-notes-preview [OPTIONS]\n\n  --data-dir PATH   Open a schema-compatible library at PATH\n  --demo            Explore disposable sample notes instead\n  --new             Create a note in this library’s main window\n  --note UUID       Open a specific note\n  --quick-note      Open or activate the small writing window\n  --reference       Graphite appearance for this launch\n  --light           Warm paper appearance for this launch\n  --capture TEXT    Save locally or through the running app (- reads stdin)\n  --request-id UUID Reuse an ID with the same capture text for safe retries\n  --capture-status UUID  Check a capture receipt without changing the library\n  --backup PATH     Save a verified backup to a new folder (close Notes first)\n  --inspect-backup PATH  Verify a completed backup without changing it\n  --restore PATH    Restore a backup to a new --data-dir (required)\n  --simulate-save-failure  Exercise save-error recovery in a scratch library\n\nDefault library: $XDG_DATA_HOME/omarchy-notes-gpui (or ~/.local/share/omarchy-notes-gpui).\nLegacy libraries are never selected automatically. Use verified backup copies when migrating.\nWindow launches forward to the running app for this library.\nCtrl Shift N opens Quick Note; Ctrl Shift O moves it into Notes.\nCtrl N creates a note, Ctrl S saves, Ctrl K searches, Ctrl \\ toggles focus.\nCtrl , opens Settings. Appearance, writing size and pane choices are remembered.\nRich editing, autosave and local storage are available.".replace("omarchy-notes-preview", env!("CARGO_BIN_NAME"))
                );
                return Ok(());
            }
            "--version" | "-V" => {
                println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--reference" => appearance = Some(notes_ui::Appearance::Reference),
            "--light" => appearance = Some(notes_ui::Appearance::Light),
            "--demo" => demo = true,
            "--quick-note" => quick = true,
            "--new" => new = true,
            "--note" => note = Some(arguments.next().context("--note needs a note UUID")?),
            "--data-dir" => {
                directory = Some(PathBuf::from(
                    arguments.next().context("--data-dir needs a path")?,
                ))
            }
            "--capture" => {
                capture = Some(
                    arguments
                        .next()
                        .context("--capture needs text, or - for stdin")?,
                )
            }
            "--request-id" => {
                request_id = Some(
                    arguments
                        .next()
                        .context("--request-id needs a lowercase UUID")?,
                )
            }
            "--capture-status" => {
                capture_status = Some(
                    arguments
                        .next()
                        .context("--capture-status needs a request UUID")?,
                )
            }
            "--simulate-save-failure" => fail = true,
            "--backup" => {
                backup = Some(PathBuf::from(
                    arguments
                        .next()
                        .context("--backup needs a new destination folder")?,
                ))
            }
            "--inspect-backup" => {
                inspect = Some(PathBuf::from(
                    arguments
                        .next()
                        .context("--inspect-backup needs a backup folder")?,
                ))
            }
            "--restore" => {
                restore = Some(PathBuf::from(arguments.next().context(
                    "--restore needs a backup folder; --data-dir must name a new destination",
                )?))
            }
            _ => bail!("Unknown option {argument}. Use --help."),
        }
    }
    ensure!(
        !demo
            || (directory.is_none()
                && capture.is_none()
                && capture_status.is_none()
                && request_id.is_none()
                && backup.is_none()
                && inspect.is_none()
                && restore.is_none()
                && !fail),
        "--demo cannot be combined with library or storage options."
    );
    ensure!(
        !(quick || new || note.is_some())
            || (!demo
                && capture.is_none()
                && capture_status.is_none()
                && backup.is_none()
                && inspect.is_none()
                && restore.is_none()),
        "Window options cannot be combined with demo, capture or backup operations."
    );
    ensure!(
        [quick, new, note.is_some()]
            .into_iter()
            .filter(|v| *v)
            .count()
            <= 1,
        "Choose only one of --new, --note or --quick-note."
    );
    ensure!(
        [
            capture.is_some(),
            capture_status.is_some(),
            backup.is_some(),
            inspect.is_some(),
            restore.is_some()
        ]
        .into_iter()
        .filter(|enabled| *enabled)
        .count()
            <= 1,
        "Choose only one of --capture, --capture-status, --backup, --inspect-backup or --restore."
    );
    ensure!(
        !fail || (backup.is_none() && inspect.is_none() && restore.is_none()),
        "--simulate-save-failure applies to editing/capture, not backup operations."
    );
    ensure!(
        inspect.is_none() || directory.is_none(),
        "--inspect-backup reads its own path; omit --data-dir."
    );
    ensure!(
        request_id.is_none() || capture.is_some(),
        "--request-id requires --capture."
    );
    ensure!(
        !fail || capture_status.is_none(),
        "--capture-status is read-only; omit --simulate-save-failure."
    );
    env_logger::init();
    if demo {
        notes_ui::run(appearance.unwrap_or(notes_ui::Appearance::Omarchy));
        return Ok(());
    }
    if let Some(path) = inspect {
        report_backup("Verified backup", notes_storage::backup::inspect(&path)?);
        return Ok(());
    }
    if let Some(source) = restore {
        let target = directory.context("Restoring requires --data-dir pointing to a new folder. Existing libraries are never replaced.")?;
        report_backup(
            "Restored library",
            notes_storage::backup::restore_to_new(&source, &target)?,
        );
        return Ok(());
    }
    let root = match directory {
        Some(path) => path,
        None => default_directory()?,
    };
    if let Some(destination) = backup {
        report_backup(
            "Saved verified backup",
            notes_storage::backup::create(&root, &destination)?,
        );
        return Ok(());
    }
    if let Some(id) = capture_status {
        return capture_command::status(&root, &id);
    }
    if let Some(text) = capture {
        return capture_command::run(&root, text, request_id, fail);
    }
    let intent = if quick {
        notes_core::launch::Intent::Quick
    } else if new {
        notes_core::launch::Intent::New
    } else if let Some(id) = note {
        notes_core::launch::Intent::Note { id }
    } else {
        notes_core::launch::Intent::Main
    };
    launch_command::run(&root, intent, appearance, fail)
}
fn report_backup(label: &str, backup: notes_storage::backup::VerifiedBackup) {
    let m = backup.manifest;
    println!(
        "{label}: {}\n{} notes · {} blobs · {} bytes · database schema {}",
        backup.path.display(),
        m.notes,
        m.blobs,
        m.bytes,
        m.schema_version
    );
}
fn default_directory() -> Result<PathBuf> {
    let base = if let Some(path) =
        std::env::var_os("XDG_DATA_HOME").filter(|p| PathBuf::from(p).is_absolute())
    {
        PathBuf::from(path)
    } else {
        PathBuf::from(std::env::var_os("HOME").context("Set HOME or pass --data-dir.")?)
            .join(".local/share")
    };
    Ok(base.join("omarchy-notes-gpui"))
}
