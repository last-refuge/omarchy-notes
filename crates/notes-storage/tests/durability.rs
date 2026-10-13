//! Kills a separate writer only after its stdout acknowledges committed writes.
use notes_core::document::{Document, Span};
use notes_storage::Store;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn writer_child() {
    let Some(path) = std::env::var_os("NOTES_DURABILITY_CHILD") else {
        return;
    };
    let mut store = Store::open(std::path::Path::new(&path)).unwrap();
    for i in 0..1000 {
        let mut body = Document::default();
        body.blocks[0].spans = vec![Span::text(format!("Acknowledged {i}"))];
        let note = store.create(body, None).unwrap();
        println!("ACK {}", note.id);
        std::io::stdout().flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn every_acknowledged_write_survives_sigkill_and_stale_lock_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_child", "--nocapture"])
        .env("NOTES_DURABILITY_CHILD", directory.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut acknowledged = vec![];
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.unwrap();
        if let Some(id) = line.strip_prefix("ACK ") {
            acknowledged.push(id.to_owned());
            if acknowledged.len() == 20 {
                break;
            }
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(acknowledged.len(), 20);
    let store = Store::open(directory.path()).unwrap();
    let notes = store.load().unwrap().notes;
    for id in acknowledged {
        assert!(
            notes.iter().any(|n| n.id == id),
            "missing acknowledged note {id}"
        );
    }
}
