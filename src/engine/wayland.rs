// SPDX-License-Identifier: GPL-3.0-only

//! Page frames shown in a Wayland subsurface, as Ladybird's own Linux UI shows them
//! (`UI/Qt/WaylandPageSurface.cpp`): each frame's dmabuf, the backing store Ladybird's Compositor
//! painted it into, is attached to a `wl_surface` of the tab's, which Rune puts under its window
//! (`NativeSurface::set_wayland_surface`). The desktop's compositor shows each frame as it is
//! committed, or scans it out, with no copy and no frame of the window's.
//!
//! The surfaces live on the windows' own display connection, in a queue of their own that a
//! thread dispatches. A backing store's buffer is made once, asynchronously, so that a dmabuf the
//! compositor can't import fails that buffer alone instead of the connection the windows use. The
//! compositor may read a buffer until it releases it; until then the store stays out of Ladybird's
//! hands.

use std::collections::HashMap;
use std::ffi::c_void;
use std::os::fd::{AsFd, OwnedFd};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use rune::native_surface::linux::WaylandSurface;
use wayland_backend::client::Backend;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::{self, WlBuffer};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::{
    self, ZwpLinuxBufferParamsV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

// Only the engine thread, which a build without the bridge lacks, asks for the device.
#[cfg(feature = "ladybird-bridge")]
mod device;
#[cfg(feature = "ladybird-bridge")]
pub(super) use device::main_device as compositor_device;

/// A backing store is rarely more than a handful; this only bounds a resize's leftovers.
const KEPT_BUFFERS: usize = 32;

/// The globals the page surfaces need, bound on the windows' connection.
struct Presenter {
    conn: Connection,
    queue: QueueHandle<State>,
    compositor: WlCompositor,
    viewporter: WpViewporter,
    dmabuf: ZwpLinuxDmabufV1,
}

static PRESENTER: OnceLock<Option<Presenter>> = OnceLock::new();

/// Whether Elaris runs under Wayland, as winit decides it: with a compositor to connect to.
pub(crate) fn session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("WAYLAND_SOCKET").is_some()
}

/// Starts showing page frames on `display`, the windows' `wl_display`. Only the first call does
/// anything; without the protocols it needs, frames go through Rune as before.
///
/// # Safety
/// `display` is the live `wl_display` of every window Elaris opens, which outlives them.
unsafe fn start(display: NonNull<c_void>) {
    PRESENTER.get_or_init(|| {
        // SAFETY: the caller promises a `wl_display` that outlives the windows; a foreign
        // backend never closes it.
        let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
        let handle = queue.handle();
        let compositor = globals
            .bind::<WlCompositor, _, _>(&handle, 4..=6, ())
            .ok()?;
        let viewporter = globals
            .bind::<WpViewporter, _, _>(&handle, 1..=1, ())
            .ok()?;
        let dmabuf = globals
            .bind::<ZwpLinuxDmabufV1, _, _>(&handle, 3..=3, ())
            .ok()?;
        std::thread::Builder::new()
            .name("elaris-wayland".to_owned())
            .spawn(move || while queue.blocking_dispatch(&mut State).is_ok() {})
            .ok()?;
        Some(Presenter {
            conn,
            queue: handle,
            compositor,
            viewporter,
            dmabuf,
        })
    });
}

/// Starts showing page frames on `window`'s display, if it is a Wayland one.
pub(crate) fn start_for(window: &rune::WindowHandle) {
    let display = window.with_raw_display_handle(|handle| match handle.as_raw() {
        rune::raw_window_handle::RawDisplayHandle::Wayland(display) => Some(display.display),
        _ => None,
    });
    if let Ok(Some(display)) = display {
        // SAFETY: winit owns the `wl_display` for the event loop, which outlives every window.
        unsafe { start(display) };
    }
}

/// Whether page frames can show in subsurfaces.
pub(crate) fn available() -> bool {
    PRESENTER.get().is_some_and(Option::is_some)
}

/// A dmabuf's buffer, made once for the backing store it belongs to.
enum Made {
    Pending,
    Ready(WlBuffer),
    Failed,
}

struct Buffer {
    key: BufferKey,
    made: Mutex<Made>,
    page: Weak<Page>,
}

/// A backing store's dmabuf and the size of the frame shown from it.
pub(super) type BufferKey = (i32, u64, u64, [u32; 2]);

/// Where a frame was shown, and what follows from that.
pub(super) enum Presented {
    /// It is on its way to the screen.
    Shown,
    /// Its buffer is still being made; the engine is woken to show it again once it is.
    Pending,
    /// The compositor can't show it; frames go through Rune instead.
    Failed,
}

/// One frame to show: its backing store's dmabuf, and how it lies there.
pub(super) struct Frame<'a> {
    pub(super) key: BufferKey,
    pub(super) fd: &'a OwnedFd,
    pub(super) offset: u32,
    pub(super) stride: u32,
    pub(super) modifier: u64,
    pub(super) format: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale: f64,
    pub(super) serial: u64,
}

/// What the compositor holds of a page's buffers, which says which frames are done with.
#[derive(Default)]
struct Held {
    /// Buffers attached and not yet released, with the newest frame each was attached for.
    buffers: HashMap<BufferKey, u64>,
    /// The newest frame attached.
    newest: u64,
}

/// What the page's buffers report to: shared by the surface and the buffers' events.
struct Page {
    held: Mutex<Held>,
    /// The newest frame serial the compositor has finished with: every older frame is off screen.
    completed: Arc<AtomicU64>,
    /// Wakes the engine thread when a frame is done with, or a buffer is made.
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Page {
    fn released(&self, key: BufferKey) {
        let done = {
            let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
            held.buffers.remove(&key);
            // The frame on screen is in a buffer the compositor holds, so every frame before the
            // oldest one it holds is done with.
            match held.buffers.values().min() {
                Some(oldest) => oldest.saturating_sub(1),
                None => held.newest,
            }
        };
        if self.completed.fetch_max(done, Ordering::AcqRel) < done {
            (self.wake)();
        }
    }
}

/// A tab's surface for its frames, with the buffers made for its backing stores.
pub(super) struct PageSurface {
    surface: WlSurface,
    viewport: WpViewport,
    buffers: Mutex<HashMap<BufferKey, Arc<Buffer>>>,
    page: Arc<Page>,
    /// The handle Rune shows the surface by, which keeps this alive while Rune uses it.
    shown: OnceLock<WaylandSurface>,
}

impl PageSurface {
    /// None until `start` found what it needs.
    pub(super) fn new(
        completed: Arc<AtomicU64>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Arc<Self>> {
        let presenter = PRESENTER.get()?.as_ref()?;
        let surface = presenter.compositor.create_surface(&presenter.queue, ());
        let viewport = presenter
            .viewporter
            .get_viewport(&surface, &presenter.queue, ());
        // The page takes no input itself: Rune's window, above it, routes it.
        let region = presenter.compositor.create_region(&presenter.queue, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        let page = Arc::new(Page {
            held: Mutex::default(),
            completed,
            wake,
        });
        Some(Arc::new(Self {
            surface,
            viewport,
            buffers: Mutex::default(),
            page,
            shown: OnceLock::new(),
        }))
    }

    /// The handle Rune shows this surface by.
    pub(super) fn handle(self: &Arc<Self>) -> Option<WaylandSurface> {
        if let Some(shown) = self.shown.get() {
            return Some(shown.clone());
        }
        let pointer = NonNull::new(self.surface.id().as_ptr().cast::<c_void>())?;
        let keep: Arc<dyn std::any::Any + Send + Sync> = self.clone();
        // SAFETY: the pointer is this surface's live `wl_surface` proxy, on the windows'
        // connection, and `keep` keeps it alive.
        let handle = unsafe { WaylandSurface::new(pointer, keep) };
        Some(self.shown.get_or_init(|| handle).clone())
    }

    /// Attaches `frame` and commits it.
    pub(super) fn present(&self, frame: &Frame<'_>) -> Presented {
        let Some(presenter) = PRESENTER.get().and_then(Option::as_ref) else {
            return Presented::Failed;
        };
        let buffer = {
            let mut buffers = self.buffers.lock().unwrap_or_else(PoisonError::into_inner);
            match buffers.get(&frame.key) {
                Some(buffer) => Arc::clone(buffer),
                None => {
                    if buffers.len() >= KEPT_BUFFERS {
                        self.forget_unheld(&mut buffers);
                    }
                    let buffer = Arc::new(Buffer {
                        key: frame.key,
                        made: Mutex::new(Made::Pending),
                        page: Arc::downgrade(&self.page),
                    });
                    make(presenter, frame, Arc::clone(&buffer));
                    buffers.insert(frame.key, buffer);
                    return Presented::Pending;
                }
            }
        };
        let made = buffer.made.lock().unwrap_or_else(PoisonError::into_inner);
        let wl_buffer = match &*made {
            Made::Pending => return Presented::Pending,
            Made::Failed => return Presented::Failed,
            Made::Ready(wl_buffer) => wl_buffer,
        };
        {
            let mut held = self
                .page
                .held
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            held.buffers.insert(frame.key, frame.serial);
            held.newest = frame.serial;
        }
        let (width, height) = (frame.width as i32, frame.height as i32);
        let logical = |pixels: u32| {
            ((f64::from(pixels) / frame.scale.max(f64::EPSILON)).round() as i32).max(1)
        };
        self.surface.attach(Some(wl_buffer), 0, 0);
        self.surface.damage_buffer(0, 0, width, height);
        self.viewport
            .set_source(0.0, 0.0, f64::from(frame.width), f64::from(frame.height));
        self.viewport
            .set_destination(logical(frame.width), logical(frame.height));
        self.surface.commit();
        drop(made);
        if presenter.conn.flush().is_err() {
            return Presented::Failed;
        }
        Presented::Shown
    }

    /// Shows nothing, so the compositor lets go of the buffer on show: the frames go to Rune now.
    pub(super) fn detach(&self) {
        self.surface.attach(None, 0, 0);
        self.surface.commit();
        if let Some(presenter) = PRESENTER.get().and_then(Option::as_ref) {
            let _ = presenter.conn.flush();
        }
    }

    /// Lets go of the buffers the compositor no longer holds.
    fn forget_unheld(&self, buffers: &mut HashMap<BufferKey, Arc<Buffer>>) {
        let held = self
            .page
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        buffers.retain(|key, buffer| {
            let keep = held.buffers.contains_key(key);
            if !keep
                && let Made::Ready(wl_buffer) =
                    &*buffer.made.lock().unwrap_or_else(PoisonError::into_inner)
            {
                wl_buffer.destroy();
            }
            keep
        });
    }
}

impl Drop for PageSurface {
    fn drop(&mut self) {
        for buffer in self
            .buffers
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            if let Made::Ready(wl_buffer) =
                &*buffer.made.lock().unwrap_or_else(PoisonError::into_inner)
            {
                wl_buffer.destroy();
            }
        }
        self.viewport.destroy();
        self.surface.destroy();
        if let Some(presenter) = PRESENTER.get().and_then(Option::as_ref) {
            let _ = presenter.conn.flush();
        }
    }
}

/// Asks the compositor for `frame`'s buffer; its answer arrives on the dispatch thread.
fn make(presenter: &Presenter, frame: &Frame<'_>, buffer: Arc<Buffer>) {
    let params = presenter.dmabuf.create_params(&presenter.queue, buffer);
    params.add(
        frame.fd.as_fd(),
        0,
        frame.offset,
        frame.stride,
        (frame.modifier >> 32) as u32,
        frame.modifier as u32,
    );
    params.create(
        frame.width as i32,
        frame.height as i32,
        frame.format,
        zwp_linux_buffer_params_v1::Flags::empty(),
    );
    let _ = presenter.conn.flush();
}

/// The dispatch thread's state: everything it needs is in the objects' own data.
struct State;

/// A buffer's data: the backing store it shows, set once it is made.
#[derive(Default)]
struct BufferData(OnceLock<Weak<Buffer>>);

impl Dispatch<ZwpLinuxBufferParamsV1, Arc<Buffer>> for State {
    fn event(
        _: &mut Self,
        params: &ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        buffer: &Arc<Buffer>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let made = match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer: wl_buffer } => {
                if let Some(data) = wl_buffer.data::<BufferData>() {
                    let _ = data.0.set(Arc::downgrade(buffer));
                }
                Made::Ready(wl_buffer)
            }
            zwp_linux_buffer_params_v1::Event::Failed => {
                eprintln!("Elaris: the compositor can't show a page's frames; Rune draws them");
                Made::Failed
            }
            _ => return,
        };
        params.destroy();
        *buffer.made.lock().unwrap_or_else(PoisonError::into_inner) = made;
        if let Some(page) = buffer.page.upgrade() {
            (page.wake)();
        }
    }

    wayland_client::event_created_child!(State, ZwpLinuxBufferParamsV1, [
        zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (WlBuffer, BufferData::default()),
    ]);
}

impl Dispatch<WlBuffer, BufferData> for State {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        event: wl_buffer::Event,
        data: &BufferData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event
            && let Some(buffer) = data.0.get().and_then(Weak::upgrade)
            && let Some(page) = buffer.page.upgrade()
        {
            page.released(buffer.key);
        }
    }
}

/// Objects whose events carry nothing the surfaces need.
macro_rules! ignore_events {
    ($($proxy:ty),*) => {$(
        impl Dispatch<$proxy, ()> for State {
            fn event(
                _: &mut Self,
                _: &$proxy,
                _: <$proxy as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignore_events!(
    WlCompositor,
    WlSurface,
    WpViewporter,
    WpViewport,
    ZwpLinuxDmabufV1,
    wayland_client::protocol::wl_region::WlRegion
);

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
