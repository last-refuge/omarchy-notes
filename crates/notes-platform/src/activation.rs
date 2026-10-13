//! Nonblocking xdg-activation over a borrowed native Wayland display.
//! The caller owns the display lifetime. This queue never reads the socket or
//! roundtrips: the host's Wayland loop reads; we dispatch only our pending events.
use anyhow::{Result, bail, ensure};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId, WaylandError};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    protocol::{wl_callback, wl_registry, wl_surface},
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1;

#[derive(Default)]
struct Globals {
    activation: Option<xdg_activation_v1::XdgActivationV1>,
    ready: bool,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == "xdg_activation_v1"
            && version >= 1
            && state.activation.is_none()
        {
            state.activation = Some(registry.bind(name, 1, qh, ()));
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for Globals {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.ready = true;
    }
}
wayland_client::delegate_noop!(Globals: ignore xdg_activation_v1::XdgActivationV1);

pub struct Pending {
    connection: Connection,
    queue: EventQueue<Globals>,
    globals: Globals,
    display: raw_window_handle::WaylandDisplayHandle,
    submitted: bool,
}
impl Pending {
    /// # Safety
    /// `display` must be a live libwayland display, read by the host event loop,
    /// and must outlive this value (including Drop). Do not change windowing
    /// backends while this value exists. The caller must also bound its lifetime.
    pub unsafe fn new(display: RawDisplayHandle) -> Result<Self> {
        let RawDisplayHandle::Wayland(display) = display else {
            bail!("Not a Wayland display.");
        };
        // SAFETY: the caller guarantees a live borrowed libwayland display. This
        // backend owns its separate queue, never the display or the host objects.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let queue = connection.new_event_queue();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        connection.display().sync(&qh, ());
        let pending = Self {
            connection,
            queue,
            globals: Globals::default(),
            display,
            submitted: false,
        };
        pending.flush()?;
        Ok(pending)
    }
    fn flush(&self) -> Result<bool> {
        match self.connection.flush() {
            Ok(()) => Ok(true),
            Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    /// Poll pending events and submit once; true means flushed, not focused.
    /// # Safety
    /// The construction lifetime contract still holds. `window` must be a live
    /// wl_surface on that same display for this call. It is never retained.
    pub unsafe fn poll(
        &mut self,
        display: RawDisplayHandle,
        window: RawWindowHandle,
        token: &notes_core::launch::ActivationToken,
    ) -> Result<bool> {
        ensure!(
            display == RawDisplayHandle::Wayland(self.display),
            "The target display changed."
        );
        let RawWindowHandle::Wayland(window) = window else {
            bail!("Not a Wayland window.");
        };
        self.queue.dispatch_pending(&mut self.globals)?;
        if !self.globals.ready {
            self.flush()?;
            return Ok(false);
        }
        let activation = self
            .globals
            .activation
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("The desktop has no activation protocol."))?;
        if !self.submitted {
            token.validate()?;
            // SAFETY: the caller guarantees this is a live wl_surface on the
            // borrowed display. from_id wraps it; it installs no listener and
            // this code never destroys or retains the host's surface proxy.
            let id = unsafe {
                ObjectId::from_ptr(
                    wl_surface::WlSurface::interface(),
                    window.surface.as_ptr().cast(),
                )
            }?;
            let surface = wl_surface::WlSurface::from_id(&self.connection, id)?;
            activation.activate(token.as_str().to_owned(), &surface);
            self.submitted = true;
        }
        self.flush()
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(activation) = self.globals.activation.take() {
            activation.destroy();
        }
        // Destroying the foreign backend releases its own registry/queue/proxies,
        // not the borrowed display or surface. Flush is nonblocking, best effort.
        let _ = self.connection.flush();
    }
}
