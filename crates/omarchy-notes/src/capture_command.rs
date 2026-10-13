use anyhow::{Context, Result, bail};
use notes_platform::instance::{Endpoint, Message, Outcome};
use notes_storage::{
    LibraryBusy, Store,
    capture::{Capture, MAX_TEXT, Receipt},
};
use std::{
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

pub fn run(root: &Path, mut text: String, request_id: Option<String>, fail: bool) -> Result<()> {
    if text == "-" {
        text.clear();
        std::io::stdin()
            .take(MAX_TEXT as u64 + 1)
            .read_to_string(&mut text)?;
    }
    let request = Capture {
        request_id: request_id.unwrap_or_else(notes_core::document::new_id),
        text,
    };
    request.validate()?;
    // Print before dispatch so even a lost process/reply leaves a lookup key in
    // the caller's output. Explicit IDs are suitable for automation/retries.
    eprintln!("Capture request: {}", request.request_id);
    let hint = format!(
        "Check --capture-status {} before retrying. Retry the original text with --request-id {} to avoid duplicates.",
        request.request_id, request.request_id
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let message = Message::capture(request.clone());
    loop {
        if root.try_exists()? {
            let endpoint = Endpoint::discover(root)?;
            if fail {
                if endpoint
                    .send(&Message::status(request.request_id.clone()))
                    .with_context(|| hint.clone())?
                    .is_some()
                {
                    bail!(
                        "The running owner's failure mode is unchanged. Start it with --simulate-save-failure to test forwarded captures."
                    );
                }
            } else if let Some(response) = endpoint.send(&message).with_context(|| hint.clone())? {
                return match response.outcome {
                    Outcome::Committed { receipt } => {
                        report(receipt);
                        Ok(())
                    }
                    Outcome::Unconfirmed { message } => {
                        bail!("Capture was not confirmed: {message} {hint}")
                    }
                    Outcome::Unknown => bail!("The owner returned no committed capture. {hint}"),
                    Outcome::Opened { .. } => {
                        bail!("A window reply cannot confirm a durable capture. {hint}")
                    }
                };
            }
        }
        match Store::open(root) {
            Ok(mut store) => {
                store.fail_saves = fail;
                let saved = store.capture(&request).with_context(|| hint.clone())?;
                report(saved.receipt);
                return Ok(());
            }
            Err(e) if e.is::<LibraryBusy>() && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(e) => return Err(e).context(hint),
        }
    }
}
pub fn status(root: &Path, id: &str) -> Result<()> {
    match notes_storage::capture::inspect(root, id)? {
        Some(receipt) => {
            report(receipt);
            Ok(())
        }
        None => bail!(
            "No committed capture found for {id} at the time of this check. A running request may still finish. Retry only with this same request ID and original text."
        ),
    }
}
fn report(receipt: Receipt) {
    println!(
        "Saved note {}\nCapture request {}",
        receipt.note_id, receipt.request_id
    );
}
