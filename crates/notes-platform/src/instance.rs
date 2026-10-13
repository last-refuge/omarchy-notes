//! Versioned, same-user Unix IPC. The Qt receipt-only endpoint is never used.
use anyhow::{Context, Result, bail, ensure};
use notes_storage::{
    Request, Worker,
    capture::{Capture, MAX_TEXT, Receipt, validate_id},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, FileTypeExt, MetadataExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub const VERSION: u32 = 1;
// JSON can expand each input byte to a six-byte escape. Still bounded before allocation.
const MAX_FRAME: usize = MAX_TEXT * 6 + 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const COMMIT_TIMEOUT: Duration = Duration::from_secs(30);
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub version: u32,
    pub request_id: String,
    pub command: Command,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Capture {
        text: String,
    },
    Status,
    Launch {
        intent: notes_core::launch::Intent,
        token: Option<notes_core::launch::ActivationToken>,
    },
}
impl Message {
    pub fn launch(
        request_id: String,
        intent: notes_core::launch::Intent,
        token: Option<notes_core::launch::ActivationToken>,
    ) -> Self {
        Self {
            version: VERSION,
            request_id,
            command: Command::Launch { intent, token },
        }
    }
    pub fn capture(request: Capture) -> Self {
        Self {
            version: VERSION,
            request_id: request.request_id,
            command: Command::Capture { text: request.text },
        }
    }
    pub fn status(id: String) -> Self {
        Self {
            version: VERSION,
            request_id: id,
            command: Command::Status,
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == VERSION,
            "Unsupported Rust instance protocol {}.",
            self.version
        );
        validate_id(&self.request_id)?;
        if let Command::Capture { text } = &self.command {
            Capture {
                request_id: self.request_id.clone(),
                text: text.clone(),
            }
            .validate()?;
        }
        if let Command::Launch { intent, token } = &self.command {
            intent.validate()?;
            if let Some(token) = token {
                token.validate()?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub request_id: String,
    pub outcome: Outcome,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Committed { receipt: Receipt },
    Unknown,
    Unconfirmed { message: String },
    Opened { window: notes_core::launch::Opened },
}
#[derive(Clone, Debug)]
pub struct Endpoint {
    library: PathBuf,
    path: PathBuf,
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
fn private_directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let m = fs::symlink_metadata(path)?;
    ensure!(
        m.is_dir() && !m.file_type().is_symlink() && m.uid() == uid() && m.mode() & 0o077 == 0,
        "The instance directory must be a private, owned directory: {}",
        path.display()
    );
    Ok(())
}
impl Endpoint {
    pub fn discover(library: &Path) -> Result<Self> {
        let runtime = if let Some(path) =
            std::env::var_os("XDG_RUNTIME_DIR").filter(|p| Path::new(p).is_absolute())
        {
            PathBuf::from(path)
        } else {
            std::env::temp_dir().join(format!("omarchy-notes-runtime-{}", uid()))
        };
        private_directory(&runtime)?;
        Self::in_runtime(library, &runtime)
    }
    /// Explicit private runtime root for embedding and isolated tests.
    pub fn in_runtime(library: &Path, runtime: &Path) -> Result<Self> {
        let m = fs::symlink_metadata(runtime)?;
        ensure!(
            runtime.is_absolute()
                && m.is_dir()
                && !m.file_type().is_symlink()
                && m.uid() == uid()
                && m.mode() & 0o077 == 0,
            "Unsafe instance runtime directory."
        );
        let library = library.canonicalize()?;
        let directory = runtime.join("omarchy-notes-rust");
        private_directory(&directory)?;
        let key = format!("{:x}", Sha256::digest(library.as_os_str().as_bytes()));
        let path = directory.join(format!("{}.sock", &key[..32]));
        ensure!(
            path.as_os_str().as_bytes().len() < 108,
            "The runtime path is too long for a local instance socket."
        );
        Ok(Self { library, path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// None means connection was absent/refused before any bytes were sent.
    /// Any other error is ambiguous: check the same request ID before retrying.
    pub fn send(&self, message: &Message) -> Result<Option<Response>> {
        message.validate()?;
        let mut stream = match UnixStream::connect(&self.path) {
            Ok(s) => s,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e).context("Could not connect to the Rust library owner."),
        };
        same_user(&stream)?;
        write_frame(&mut stream, &serde_json::to_vec(message)?, IO_TIMEOUT)?;
        let response: Response = serde_json::from_slice(&read_frame(
            &mut stream,
            64 * 1024,
            COMMIT_TIMEOUT + IO_TIMEOUT,
        )?)?;
        ensure!(
            response.version == VERSION && response.request_id == message.request_id,
            "The instance reply does not match this request; its outcome is unknown."
        );
        if let Outcome::Committed { receipt } = &response.outcome {
            ensure!(
                !matches!(message.command, Command::Launch { .. }),
                "A storage receipt cannot confirm a window launch."
            );
            ensure!(
                receipt.request_id == message.request_id,
                "The committed receipt belongs to another request."
            );
            validate_id(&receipt.note_id)?;
        }
        if let Outcome::Opened { window } = &response.outcome {
            let Command::Launch { intent, .. } = &message.command else {
                bail!("A window reply cannot confirm a durable capture.");
            };
            ensure!(
                &window.intent == intent,
                "The window reply describes a different action."
            );
            if let Some(id) = &window.note_id {
                validate_id(id)?;
            }
            if matches!(
                intent,
                notes_core::launch::Intent::New | notes_core::launch::Intent::Quick
            ) {
                ensure!(
                    window.note_id.is_some(),
                    "The owner did not identify the opened note."
                );
            }
            if let notes_core::launch::Intent::Note { id } = intent {
                ensure!(
                    window.note_id.as_ref() == Some(id),
                    "The owner opened a different note."
                );
            }
        }
        Ok(Some(response))
    }
}
fn same_user(stream: &UnixStream) -> Result<()> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of_val(&credentials) as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    ensure!(
        result == 0 && credentials.uid == uid(),
        "The instance peer is not owned by this user."
    );
    Ok(())
}
fn read_exact(stream: &mut UnixStream, buffer: &mut [u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    while offset < buffer.len() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("Instance read timed out; check the request ID before retrying.")?;
        stream.set_read_timeout(Some(remaining))?;
        let n = stream.read(&mut buffer[offset..])?;
        ensure!(
            n != 0,
            "The instance closed before replying; check the request ID before retrying."
        );
        offset += n;
    }
    Ok(())
}
fn read_frame(stream: &mut UnixStream, limit: usize, timeout: Duration) -> Result<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut size = [0u8; 4];
    read_exact(stream, &mut size, deadline)?;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(
        size > 0 && size <= limit,
        "Instance frame exceeds its size limit."
    );
    // Grow only as bytes arrive; an advertised maximum cannot reserve 192 MiB.
    let mut out = Vec::with_capacity(size.min(64 * 1024));
    let mut chunk = [0u8; 64 * 1024];
    while out.len() < size {
        let count = (size - out.len()).min(chunk.len());
        read_exact(stream, &mut chunk[..count], deadline)?;
        out.extend_from_slice(&chunk[..count]);
    }
    Ok(out)
}
fn write_frame(stream: &mut UnixStream, bytes: &[u8], timeout: Duration) -> Result<()> {
    ensure!(
        bytes.len() <= MAX_FRAME,
        "Instance frame exceeds its size limit."
    );
    let deadline = Instant::now() + timeout;
    let header = (bytes.len() as u32).to_be_bytes();
    for part in [&header[..], bytes] {
        let mut offset = 0;
        while offset < part.len() {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("Instance write timed out; check the request ID before retrying.")?;
            stream.set_write_timeout(Some(remaining))?;
            let n = stream.write(&part[offset..])?;
            ensure!(
                n > 0,
                "The instance stopped accepting bytes; check the request ID before retrying."
            );
            offset += n;
        }
    }
    Ok(())
}
fn handle(
    mut stream: UnixStream,
    requests: &mpsc::SyncSender<Request>,
    launches: Option<&crate::launch::Sender>,
) -> Result<()> {
    same_user(&stream)?;
    let request: Message =
        serde_json::from_slice(&read_frame(&mut stream, MAX_FRAME, IO_TIMEOUT)?)?;
    let outcome = if let Err(e) = request.validate() {
        Outcome::Unconfirmed {
            message: e.to_string(),
        }
    } else {
        match request.command {
            Command::Launch { intent, token } => {
                let (tx, rx) = mpsc::sync_channel(1);
                let launch = crate::launch::Request {
                    id: request.request_id.clone(),
                    intent,
                    token,
                    expires: Instant::now() + COMMIT_TIMEOUT,
                    reply: tx,
                };
                match launches {
                    None => Outcome::Unconfirmed { message: "This owner supports capture only. Open the Rust desktop app to receive window launches.".into() },
                    Some(sender) => match sender.try_send(launch) {
                        Err(_) => Outcome::Unconfirmed { message: "The app is busy or closing. This window request was not queued.".into() },
                        Ok(()) => match rx.recv_timeout(COMMIT_TIMEOUT) {
                            Ok(Ok(window)) => Outcome::Opened { window },
                            Ok(Err(message)) => Outcome::Unconfirmed { message },
                            Err(_) => Outcome::Unconfirmed { message: "Opening the window is not confirmed. Check the running app before repeating a New Note request; it may still finish.".into() },
                        }
                    }
                }
            }
            Command::Capture { text } => {
                let (tx, rx) = mpsc::sync_channel(1);
                match requests.try_send(Request::Capture{request:Capture{request_id:request.request_id.clone(),text},reply:tx}) {
                    Ok(())=>match rx.recv_timeout(COMMIT_TIMEOUT) {
                        Ok(Ok(receipt))=>Outcome::Committed{receipt},
                        Ok(Err(message))=>Outcome::Unconfirmed{message},
                        Err(_)=>Outcome::Unconfirmed{message:"Capture completion is unknown. Check this request ID before retrying; it may still commit.".into()}
                    },
                    Err(_)=>Outcome::Unconfirmed{message:"Storage is busy or closing. Check the request ID, then retry with the same ID and text.".into()}
                }
            }
            Command::Status => {
                let (tx, rx) = mpsc::sync_channel(1);
                match requests.try_send(Request::CaptureStatus{request_id:request.request_id.clone(),reply:tx}) {
                    Ok(())=>match rx.recv_timeout(COMMIT_TIMEOUT){Ok(Ok(Some(receipt)))=>Outcome::Committed{receipt},Ok(Ok(None))=>Outcome::Unknown,Ok(Err(message))=>Outcome::Unconfirmed{message},Err(_)=>Outcome::Unconfirmed{message:"Storage is still busy. The capture outcome is unknown; check again.".into()}},
                    Err(_)=>Outcome::Unconfirmed{message:"Storage is busy or closing; check again.".into()}
                }
            }
        }
    };
    write_frame(
        &mut stream,
        &serde_json::to_vec(&Response {
            version: VERSION,
            request_id: request.request_id,
            outcome,
        })?,
        IO_TIMEOUT,
    )
}
struct SocketFile {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for SocketFile {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.dev() == self.device && m.ino() == self.inode && m.file_type().is_socket()
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub struct Server {
    stop: Arc<AtomicBool>,
    listener: Option<thread::JoinHandle<()>>,
    wake: UnixStream,
}
impl Server {
    /// The worker is the proof of exclusive library ownership. Only its canonical
    /// library's endpoint may be bound, and a responding socket is never removed.
    pub fn bind(worker: &Worker, endpoint: Endpoint) -> Result<Self> {
        Self::bind_with_launches(worker, endpoint, None)
    }
    pub fn bind_with_launches(
        worker: &Worker,
        endpoint: Endpoint,
        launches: Option<crate::launch::Sender>,
    ) -> Result<Self> {
        ensure!(
            worker.root() == endpoint.library,
            "The endpoint belongs to another library."
        );
        if let Ok(meta) = fs::symlink_metadata(&endpoint.path) {
            ensure!(
                meta.file_type().is_socket() && meta.uid() == uid(),
                "An unrelated file occupies the instance endpoint; it was not removed."
            );
            match UnixStream::connect(&endpoint.path) {
                Ok(_) => bail!("Another server responds at this endpoint; it was not replaced."),
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                    let current = fs::symlink_metadata(&endpoint.path)?;
                    ensure!(
                        current.ino() == meta.ino() && current.dev() == meta.dev(),
                        "Instance endpoint changed during startup."
                    );
                    fs::remove_file(&endpoint.path)?;
                }
                Err(e) => {
                    return Err(e).context("Could not inspect the existing instance endpoint.");
                }
            }
        }
        let listener = UnixListener::bind(&endpoint.path)?;
        let meta = fs::symlink_metadata(&endpoint.path)?;
        let file = SocketFile {
            path: endpoint.path,
            device: meta.dev(),
            inode: meta.ino(),
        };
        listener.set_nonblocking(true)?;
        let (wake, wake_reader) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (jobs, rx) = mpsc::sync_channel::<UnixStream>(8);
        let rx = Arc::new(Mutex::new(rx));
        // Two input workers bound active parsed requests; eight queued sockets
        // have no allocated input bodies. All mutations use the one DB writer.
        for n in 0..2 {
            let receiver = rx.clone();
            let requests = worker.requests.clone();
            let stop = stop.clone();
            let launches = launches.clone();
            thread::Builder::new()
                .name(format!("notes-ipc-{n}"))
                .spawn(move || {
                    loop {
                        let socket = receiver.lock().unwrap().recv();
                        let Ok(socket) = socket else {
                            break;
                        };
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let _ = handle(socket, &requests, launches.as_ref());
                    }
                })?;
        }
        let listener = thread::Builder::new()
            .name("notes-instance".into())
            .spawn(move || {
                let _file = file;
                let mut descriptors = [
                    libc::pollfd {
                        fd: listener.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: wake_reader.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                while !stopping.load(Ordering::Relaxed) {
                    // Sleep in the kernel while idle. Shutdown wakes this poll;
                    // there is no timer repeatedly waking the desktop process.
                    let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, -1) };
                    if ready < 0 {
                        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                        {
                            continue;
                        }
                        break;
                    }
                    if descriptors[1].revents != 0 || stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = jobs.try_send(stream);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            stop,
            listener: Some(listener),
            wake,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.wake.write(&[1]);
        if let Some(thread) = self.listener.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests;
