//! Explicit copy/paste only. No clipboard history, polling of contents, or persistence.
//! WLR data-control supplies multiple MIME offers missing from the pinned GPUI API.
use anyhow::{Context, Result, ensure};
use notes_exchange::{FRAGMENT_MIME, MAX_CLIPBOARD};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, event_created_child,
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as device, zwlr_data_control_manager_v1 as manager,
    zwlr_data_control_offer_v1 as offer, zwlr_data_control_source_v1 as source,
};
pub type Offers = BTreeMap<String, Vec<u8>>;
#[derive(Clone, Debug)]
pub struct Content {
    pub mime: String,
    pub bytes: Vec<u8>,
}
type Reply<T> = mpsc::SyncSender<std::result::Result<T, String>>;
enum Command {
    Copy(Offers, Reply<()>),
    Paste(bool, Reply<Content>),
}
#[derive(Clone)]
pub struct Clipboard {
    commands: mpsc::SyncSender<Command>,
}
impl Clipboard {
    pub fn connect() -> Result<Self> {
        let (commands, receive) = mpsc::sync_channel(8);
        let (ready, initialized) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("notes-clipboard".into())
            .spawn(move || {
                let result = connect();
                let (connection, mut queue, mut state) = match result {
                    Ok(v) => {
                        let _ = ready.send(Ok(()));
                        v
                    }
                    Err(e) => {
                        let _ = ready.send(Err(format!("{e:#}")));
                        return;
                    }
                };
                loop {
                    match receive.try_recv() {
                        Ok(Command::Copy(offers, reply)) => {
                            let result =
                                write_selection(&connection, &mut queue, &mut state, offers);
                            let _ = reply.send(result.map_err(|e| format!("{e:#}")));
                        }
                        Ok(Command::Paste(plain, reply)) => {
                            let result = read_selection(&connection, &mut queue, &mut state, plain);
                            let _ = reply.send(result.map_err(|e| format!("{e:#}")));
                        }
                        Err(mpsc::TryRecvError::Disconnected) => break,
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                    if queue.dispatch_pending(&mut state).is_err() || connection.flush().is_err() {
                        break;
                    }
                    if let Some(guard) = queue.prepare_read() {
                        let fd = guard.connection_fd();
                        let mut fds = [rustix::event::PollFd::new(
                            &fd,
                            rustix::event::PollFlags::IN,
                        )];
                        match rustix::event::poll(
                            &mut fds,
                            Some(&rustix::event::Timespec {
                                tv_sec: 0,
                                tv_nsec: 20_000_000,
                            }),
                        ) {
                            Ok(n) if n > 0 => {
                                if guard.read().is_err() {
                                    break;
                                }
                            }
                            Ok(_) => drop(guard),
                            Err(_) => break,
                        }
                    }
                }
            })?;
        initialized
            .recv_timeout(Duration::from_secs(5))
            .context("Clipboard initialization timed out.")?
            .map_err(anyhow::Error::msg)?;
        Ok(Self { commands })
    }
    /// Blocking methods must run on a background executor, never during render/input.
    pub fn copy(&self, offers: Offers) -> Result<()> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Copy(offers, tx))
            .context("The clipboard is busy. Try copying again.")?;
        rx.recv_timeout(Duration::from_secs(8))
            .context("Clipboard copy timed out.")?
            .map_err(anyhow::Error::msg)
    }
    pub fn paste(&self, plain: bool) -> Result<Content> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Paste(plain, tx))
            .context("The clipboard is busy. Try pasting again.")?;
        rx.recv_timeout(Duration::from_secs(8))
            .context("Clipboard paste timed out.")?
            .map_err(anyhow::Error::msg)
    }
}
struct Offer {
    proxy: offer::ZwlrDataControlOfferV1,
    mimes: Vec<String>,
}
#[derive(Default)]
struct State {
    manager: Option<manager::ZwlrDataControlManagerV1>,
    seat: Option<wl_seat::WlSeat>,
    device: Option<device::ZwlrDataControlDeviceV1>,
    offers: Vec<Offer>,
    current: Option<offer::ZwlrDataControlOfferV1>,
    source: Option<source::ZwlrDataControlSourceV1>,
    own: Arc<Offers>,
    marker: String,
}
fn connect() -> Result<(Connection, wayland_client::EventQueue<State>, State)> {
    let connection = Connection::connect_to_env().context("Wayland clipboard is unavailable.")?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state)?;
    let manager = state.manager.as_ref().context(
        "This compositor does not offer WLR data-control. Rich clipboard transport is unavailable.",
    )?;
    let seat = state.seat.as_ref().context("No Wayland seat.")?;
    state.device = Some(manager.get_data_device(seat, &qh, ()));
    state.marker = format!(
        "application/x-omarchy-notes-owner-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    queue.roundtrip(&mut state)?;
    Ok((connection, queue, state))
}
fn write_selection(
    connection: &Connection,
    queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
    mut values: Offers,
) -> Result<()> {
    ensure!(
        values.len() <= 12 && values.values().all(|v| v.len() <= MAX_CLIPBOARD),
        "Clipboard data exceeds the transfer limit."
    );
    values.insert(state.marker.clone(), vec![]);
    let values = Arc::new(values);
    let source = state
        .manager
        .as_ref()
        .unwrap()
        .create_data_source(&queue.handle(), values.clone());
    for mime in values.keys() {
        source.offer(mime.clone());
    }
    state.device.as_ref().unwrap().set_selection(Some(&source));
    state.source = Some(source);
    state.own = values;
    connection.flush()?;
    queue.roundtrip(state)?;
    Ok(())
}
fn read_selection(
    connection: &Connection,
    queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
    plain: bool,
) -> Result<Content> {
    queue.roundtrip(state)?;
    let current = state.current.as_ref().context("The clipboard is empty.")?;
    let offered = state
        .offers
        .iter()
        .find(|o| &o.proxy == current)
        .context("The clipboard offer expired.")?;
    let preferences = if plain {
        vec!["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"]
    } else {
        vec![
            FRAGMENT_MIME,
            "text/uri-list",
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/gif",
            "text/html",
            "text/plain;charset=utf-8",
            "text/plain",
            "UTF8_STRING",
        ]
    };
    let mime = preferences
        .into_iter()
        .find(|m| offered.mimes.iter().any(|v| v == m))
        .context("No supported text, image, or file data is available.")?
        .to_string();
    if offered.mimes.contains(&state.marker)
        && let Some(bytes) = state.own.get(&mime)
    {
        return Ok(Content {
            mime,
            bytes: bytes.clone(),
        });
    }
    let (mut read, write) = UnixStream::pair()?;
    read.set_read_timeout(Some(Duration::from_secs(3)))?;
    offered.proxy.receive(mime.clone(), write.as_fd());
    drop(write);
    connection.flush()?;
    let mut bytes = vec![];
    Read::by_ref(&mut read)
        .take((MAX_CLIPBOARD + 1) as u64)
        .read_to_end(&mut bytes)
        .context("Could not read clipboard data before the transfer timeout.")?;
    ensure!(
        bytes.len() <= MAX_CLIPBOARD,
        "Clipboard data exceeds the transfer limit."
    );
    Ok(Content { mime, bytes })
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = e
        {
            match interface.as_str() {
                "zwlr_data_control_manager_v1" => {
                    s.manager = Some(r.bind(name, version.min(2), q, ()))
                }
                "wl_seat" if s.seat.is_none() => s.seat = Some(r.bind(name, 1, q, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<device::ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &device::ZwlrDataControlDeviceV1,
        e: device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            device::Event::DataOffer { id } => s.offers.push(Offer {
                proxy: id,
                mimes: vec![],
            }),
            device::Event::Selection { id } => {
                let old = s.current.take();
                s.current = id.clone();
                if let Some(old) = old
                    && Some(&old) != id.as_ref()
                {
                    s.offers.retain(|o| o.proxy != old);
                    old.destroy();
                }
                if !s
                    .offers
                    .iter()
                    .any(|o| Some(&o.proxy) == id.as_ref() && o.mimes.contains(&s.marker))
                {
                    s.own = Arc::new(Offers::new());
                }
            }
            device::Event::PrimarySelection { id: Some(id) } => {
                s.offers.retain(|o| o.proxy != id);
                id.destroy();
            }
            _ => {}
        }
    }
    event_created_child!(State,device::ZwlrDataControlDeviceV1,[0=>(offer::ZwlrDataControlOfferV1,())]);
}
impl Dispatch<offer::ZwlrDataControlOfferV1, ()> for State {
    fn event(
        s: &mut Self,
        p: &offer::ZwlrDataControlOfferV1,
        e: offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let offer::Event::Offer { mime_type } = e
            && let Some(o) = s.offers.iter_mut().find(|o| &o.proxy == p)
            && o.mimes.len() < 128
            && mime_type.len() <= 512
        {
            o.mimes.push(mime_type);
        }
    }
}
impl Dispatch<source::ZwlrDataControlSourceV1, Arc<Offers>> for State {
    fn event(
        _: &mut Self,
        p: &source::ZwlrDataControlSourceV1,
        e: source::Event,
        values: &Arc<Offers>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            source::Event::Send { mime_type, fd } => {
                if let Some(bytes) = values.get(&mime_type) {
                    let mut file = File::from(fd);
                    let _ = write_bounded(&mut file, bytes);
                }
            }
            source::Event::Cancelled => p.destroy(),
            _ => {}
        }
    }
}
fn write_bounded(file: &mut File, mut bytes: &[u8]) -> Result<()> {
    let flags = rustix::fs::fcntl_getfl(&*file)?;
    rustix::fs::fcntl_setfl(&*file, flags | rustix::fs::OFlags::NONBLOCK)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while !bytes.is_empty() {
        ensure!(Instant::now() < deadline, "Clipboard receiver timed out.");
        match file.write(bytes) {
            Ok(0) => anyhow::bail!("Clipboard receiver closed."),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let mut fds = [rustix::event::PollFd::new(
                    &*file,
                    rustix::event::PollFlags::OUT,
                )];
                rustix::event::poll(
                    &mut fds,
                    Some(&rustix::event::Timespec {
                        tv_sec: 0,
                        tv_nsec: 50_000_000,
                    }),
                )?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
delegate_noop!(State:ignore wl_seat::WlSeat);
delegate_noop!(State:ignore manager::ZwlrDataControlManagerV1);
