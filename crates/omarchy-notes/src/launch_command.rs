//! Decide ownership before GPU startup. Never fall back after an ambiguous send.
use anyhow::{Context, Result, bail, ensure};
use notes_core::launch::{Activation, ActivationToken, Intent};
use notes_platform::instance::{Endpoint, Message, Outcome};
use notes_storage::{LibraryBusy, Store, Worker};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub fn run(
    root: &Path,
    intent: Intent,
    appearance: Option<notes_ui::Appearance>,
    fail: bool,
) -> Result<()> {
    intent.validate()?;
    if matches!(intent, Intent::Note { .. }) {
        ensure!(
            root.try_exists()?,
            "That library does not exist. No note was opened."
        );
    }
    let token = std::env::var("XDG_ACTIVATION_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .map(ActivationToken::new)
        .transpose()?;
    let message = Message::launch(notes_core::document::new_id(), intent.clone(), token);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if root.try_exists()? {
            let endpoint = Endpoint::discover(root)?;
            if fail {
                if endpoint.send(&Message::status(message.request_id.clone()))?.is_some() {
                    bail!("The running owner's failure mode is unchanged. Start it with --simulate-save-failure to test window creation and saving.");
                }
            } else if let Some(response) = endpoint.send(&message).context("The window request outcome is unknown. Check the running app before repeating --new; no second process was started.")? {
                return match response.outcome {
                    Outcome::Opened { window } => {
                        if window.activation == Activation::Unavailable { eprintln!("The window is open, but its activation token could not be submitted. Select Notes from your desktop."); }
                        match window.note_id {
                            Some(id) => println!("Opened note {id}"),
                            None => println!("Opened Notes"),
                        }
                        Ok(())
                    }
                    Outcome::Unconfirmed { message } => bail!("Window opening was not confirmed: {message}"),
                    _ => bail!("The owner did not confirm a window. No second process was started."),
                };
            }
        }
        match Store::open(root) {
            Ok(mut store) => {
                store.fail_saves = fail;
                let (worker, library) = Worker::from_store(store)?;
                // GPUI consumes the original process's startup token when it
                // connects. Only forwarded requests use the explicit adapter.
                return notes_ui::run_owned_library(appearance, worker, library, intent);
            }
            Err(e) if e.is::<LibraryBusy>() && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(e) => {
                return Err(e).context("Could not open or forward this library's window request.");
            }
        }
    }
}
