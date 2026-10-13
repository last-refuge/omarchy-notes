use super::*;
use notes_core::document::new_id;
use notes_core::launch::{Activation, ActivationToken, Intent, Opened};
fn private_temp() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}
fn setup() -> (tempfile::TempDir, Worker, Endpoint, Server) {
    let t = private_temp();
    let root = t.path().join("library");
    let (w, _) = Worker::start(root, false).unwrap();
    let e = Endpoint::in_runtime(w.root(), t.path()).unwrap();
    let s = Server::bind(&w, e.clone()).unwrap();
    (t, w, e, s)
}

#[test]
fn launch_replies_wait_for_ui_confirmation_and_do_not_claim_a_capture() {
    let t = private_temp();
    let (worker, _) = Worker::start(t.path().join("library"), false).unwrap();
    let endpoint = Endpoint::in_runtime(worker.root(), t.path()).unwrap();
    let (tx, rx) = crate::launch::channel();
    let _server = Server::bind_with_launches(&worker, endpoint.clone(), Some(tx)).unwrap();
    let message = Message::launch(
        new_id(),
        Intent::New,
        Some(ActivationToken::new("test-token".into()).unwrap()),
    );
    let id = message.request_id.clone();
    let (reply_tx, reply_rx) = mpsc::channel();
    let thread = thread::spawn(move || reply_tx.send(endpoint.send(&message)).unwrap());
    let request = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(reply_rx.try_recv().is_err());
    assert_eq!(request.id, id);
    assert_eq!(request.intent, Intent::New);
    assert_eq!(request.token.unwrap().as_str(), "test-token");
    assert!(worker.events.try_recv().is_err());
    request
        .reply
        .send(Ok(Opened {
            intent: Intent::New,
            note_id: Some(new_id()),
            activation: Activation::TokenRequested,
        }))
        .unwrap();
    assert!(matches!(
        reply_rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap()
            .unwrap()
            .outcome,
        Outcome::Opened { .. }
    ));
    assert!(
        notes_storage::capture::inspect(worker.root(), &id)
            .unwrap()
            .is_none()
    );
    thread.join().unwrap();
}

#[test]
fn launches_to_capture_only_or_full_ui_queue_are_rejected_without_writing() {
    let (_t, worker, endpoint, _server) = setup();
    let message = Message::launch(new_id(), Intent::New, None);
    assert!(matches!(
        endpoint.send(&message).unwrap().unwrap().outcome,
        Outcome::Unconfirmed { .. }
    ));
    assert!(worker.events.try_recv().is_err());
    let (tx, rx) = crate::launch::channel();
    for _ in 0..8 {
        let (reply, _) = mpsc::sync_channel(1);
        tx.try_send(crate::launch::Request {
            id: new_id(),
            intent: Intent::Main,
            token: None,
            expires: Instant::now() + COMMIT_TIMEOUT,
            reply,
        })
        .unwrap();
    }
    let (writer, events) = mpsc::sync_channel(1);
    let (mut client, server) = UnixStream::pair().unwrap();
    let task = thread::spawn(move || handle(server, &writer, Some(&tx)));
    write_frame(
        &mut client,
        &serde_json::to_vec(&message).unwrap(),
        IO_TIMEOUT,
    )
    .unwrap();
    let response: Response =
        serde_json::from_slice(&read_frame(&mut client, 4096, IO_TIMEOUT).unwrap()).unwrap();
    assert!(matches!(response.outcome, Outcome::Unconfirmed { .. }));
    assert!(events.try_recv().is_err());
    assert_eq!(rx.try_iter().count(), 8);
    task.join().unwrap().unwrap();
}

#[test]
fn window_validation_rejects_invalid_ids_tokens_and_unrelated_success() {
    let mut message = Message::launch(new_id(), Intent::Note { id: "bad".into() }, None);
    assert!(message.validate().is_err());
    message.command = Command::Launch {
        intent: Intent::Main,
        token: Some(serde_json::from_str("\"bad\\nvalue\"").unwrap()),
    };
    assert!(message.validate().is_err());
    for invalid in 0..4 {
        let t = private_temp();
        let (worker, _) = Worker::start(t.path().join("library"), false).unwrap();
        let endpoint = Endpoint::in_runtime(worker.root(), t.path()).unwrap();
        let listener = UnixListener::bind(endpoint.path()).unwrap();
        let id = new_id();
        let reply_id = id.clone();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_frame(&mut stream, 4096, IO_TIMEOUT).unwrap();
            let outcome = if invalid == 0 {
                Outcome::Committed {
                    receipt: Receipt {
                        request_id: reply_id.clone(),
                        note_id: new_id(),
                        committed_at: 0,
                    },
                }
            } else {
                Outcome::Opened {
                    window: Opened {
                        intent: if invalid == 1 {
                            Intent::Quick
                        } else {
                            Intent::New
                        },
                        note_id: if invalid == 2 { None } else { Some(new_id()) },
                        activation: Activation::Requested,
                    },
                }
            };
            write_frame(
                &mut stream,
                &serde_json::to_vec(&Response {
                    version: VERSION,
                    request_id: reply_id,
                    outcome,
                })
                .unwrap(),
                IO_TIMEOUT,
            )
            .unwrap();
        });
        let message = if invalid == 3 {
            capture(&id, "Cannot confuse opening with saving")
        } else {
            Message::launch(id, Intent::New, None)
        };
        assert!(endpoint.send(&message).is_err());
        peer.join().unwrap();
        assert!(worker.events.try_recv().is_err());
    }
}
fn capture(id: &str, text: &str) -> Message {
    Message::capture(Capture {
        request_id: id.into(),
        text: text.into(),
    })
}
fn receipt(response: Response) -> Receipt {
    match response.outcome {
        Outcome::Committed { receipt } => receipt,
        _ => panic!("capture did not commit"),
    }
}
#[test]
fn server_captures_once_and_reports_only_durable_results() {
    let (_t, w, e, _s) = setup();
    let id = new_id();
    let saved = receipt(e.send(&capture(&id, "A thought 👩‍💻")).unwrap().unwrap());
    assert_eq!(
        notes_storage::capture::inspect(w.root(), &id).unwrap(),
        Some(saved.clone())
    );
    assert_eq!(
        receipt(e.send(&capture(&id, "A thought 👩‍💻")).unwrap().unwrap()),
        saved
    );
    assert_eq!(
        receipt(e.send(&Message::status(id.clone())).unwrap().unwrap()),
        saved
    );
    assert!(matches!(
        e.send(&capture(&id, "different text"))
            .unwrap()
            .unwrap()
            .outcome,
        Outcome::Unconfirmed { .. }
    ));
    assert!(matches!(
        e.send(&Message::status(new_id())).unwrap().unwrap().outcome,
        Outcome::Unknown
    ));
    assert!(matches!(
        w.events.recv_timeout(Duration::from_secs(2)).unwrap(),
        notes_storage::Event::Captured { .. }
    ));
    assert!(w.events.try_recv().is_err());
}
#[test]
fn lost_reply_can_be_queried_and_retried_without_duplicate() {
    let (_t, w, e, _s) = setup();
    let id = new_id();
    let msg = capture(&id, "Keep this even if the client disappears");
    let mut stream = UnixStream::connect(e.path()).unwrap();
    write_frame(&mut stream, &serde_json::to_vec(&msg).unwrap(), IO_TIMEOUT).unwrap();
    drop(stream);
    assert!(matches!(
        w.events.recv_timeout(Duration::from_secs(3)).unwrap(),
        notes_storage::Event::Captured { .. }
    ));
    let original = receipt(e.send(&Message::status(id.clone())).unwrap().unwrap());
    assert_eq!(receipt(e.send(&msg).unwrap().unwrap()), original);
    assert!(w.events.try_recv().is_err());
}
#[test]
fn malformed_oversized_future_and_unrelated_endpoints_are_not_actions() {
    let (t, w, e, s) = setup();
    for payload in [
        b"{broken".to_vec(),
        serde_json::to_vec(&Message {
            version: 99,
            request_id: new_id(),
            command: Command::Capture {
                text: "must not save".into(),
            },
        })
        .unwrap(),
    ] {
        let mut stream = UnixStream::connect(e.path()).unwrap();
        write_frame(&mut stream, &payload, IO_TIMEOUT).unwrap();
        let _ = read_frame(&mut stream, 1024, IO_TIMEOUT);
    }
    let mut stream = UnixStream::connect(e.path()).unwrap();
    stream.write_all(&u32::MAX.to_be_bytes()).unwrap();
    assert!(read_frame(&mut stream, 1024, IO_TIMEOUT).is_err());
    assert!(w.events.try_recv().is_err());
    assert!(Server::bind(&w, e.clone()).is_err());
    assert!(e.path().exists());
    drop(s);
    assert!(!e.path().exists());
    fs::write(e.path(), b"unrelated").unwrap();
    assert!(Server::bind(&w, e.clone()).is_err());
    assert_eq!(fs::read(e.path()).unwrap(), b"unrelated");
    let other = t.path().join("another");
    fs::create_dir(&other).unwrap();
    let different = Endpoint::in_runtime(&other, t.path()).unwrap();
    assert!(Server::bind(&w, different).is_err());
}
#[test]
fn stale_owned_socket_is_reclaimed_but_symlinks_and_public_runtime_are_refused() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let t = private_temp();
    let (w, _) = Worker::start(t.path().join("library"), false).unwrap();
    let e = Endpoint::in_runtime(w.root(), t.path()).unwrap();
    drop(UnixListener::bind(e.path()).unwrap());
    let s = Server::bind(&w, e.clone()).unwrap();
    drop(s);
    symlink(t.path().join("target"), e.path()).unwrap();
    assert!(Server::bind(&w, e.clone()).is_err());
    assert!(
        fs::symlink_metadata(e.path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let public = t.path().join("public");
    fs::create_dir(&public).unwrap();
    fs::set_permissions(&public, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Endpoint::in_runtime(w.root(), &public).is_err());
}
#[test]
fn stopped_or_saturated_writer_never_reports_saved() {
    let t = private_temp();
    let (w, _) = Worker::start(t.path().join("library"), true).unwrap();
    let e = Endpoint::in_runtime(w.root(), t.path()).unwrap();
    let _s = Server::bind(&w, e.clone()).unwrap();
    let id = new_id();
    assert!(matches!(
        e.send(&capture(&id, "will fail")).unwrap().unwrap().outcome,
        Outcome::Unconfirmed { .. }
    ));
    assert!(
        notes_storage::capture::inspect(w.root(), &id)
            .unwrap()
            .is_none()
    );
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    let (mut client, server) = UnixStream::pair().unwrap();
    let task = thread::spawn(move || handle(server, &tx, None));
    write_frame(
        &mut client,
        &serde_json::to_vec(&capture(&new_id(), "will not run")).unwrap(),
        IO_TIMEOUT,
    )
    .unwrap();
    let response: Response =
        serde_json::from_slice(&read_frame(&mut client, 4096, IO_TIMEOUT).unwrap()).unwrap();
    assert!(matches!(response.outcome, Outcome::Unconfirmed { .. }));
    task.join().unwrap().unwrap();
}

#[test]
fn full_writer_queue_and_receipt_only_or_mismatched_replies_are_unconfirmed() {
    let (tx, _rx) = mpsc::sync_channel(1);
    let (reply, _answer) = mpsc::sync_channel(1);
    tx.send(Request::CaptureStatus {
        request_id: new_id(),
        reply,
    })
    .unwrap();
    let (mut client, server) = UnixStream::pair().unwrap();
    let task = thread::spawn(move || handle(server, &tx, None));
    write_frame(
        &mut client,
        &serde_json::to_vec(&capture(&new_id(), "Queue is full")).unwrap(),
        IO_TIMEOUT,
    )
    .unwrap();
    let response: Response =
        serde_json::from_slice(&read_frame(&mut client, 4096, IO_TIMEOUT).unwrap()).unwrap();
    assert!(matches!(response.outcome, Outcome::Unconfirmed { .. }));
    task.join().unwrap().unwrap();

    for mismatch in [false, true] {
        let t = private_temp();
        let (w, _) = Worker::start(t.path().join("library"), false).unwrap();
        let endpoint = Endpoint::in_runtime(w.root(), t.path()).unwrap();
        let listener = UnixListener::bind(endpoint.path()).unwrap();
        let peer = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _ = read_frame(&mut socket, 4096, IO_TIMEOUT).unwrap();
            if mismatch {
                let answer = Response {
                    version: VERSION,
                    request_id: new_id(),
                    outcome: Outcome::Unknown,
                };
                write_frame(
                    &mut socket,
                    &serde_json::to_vec(&answer).unwrap(),
                    IO_TIMEOUT,
                )
                .unwrap();
            } else {
                socket.write_all(b"ok\n").unwrap();
            }
        });
        assert!(
            endpoint
                .send(&capture(&new_id(), "Never acknowledge receipt as saved"))
                .is_err()
        );
        peer.join().unwrap();
        assert!(w.events.try_recv().is_err());
    }
}
