// SPDX-License-Identifier: GPL-3.0-only

//! Page frames Rune's GPU draws in place, with no CPU copy.
//!
//! macOS: the bridge copies each frame on the GPU into an IOSurface of its own and the surface is
//! wrapped for Rune. Rune keeps every surface it has drawn pinned, so it never gets Ladybird's own
//! backing stores, which the Compositor does not repaint while another process holds them. The
//! bridge reuses a surface once Rune reports it has drawn a newer frame.
//!
//! Linux: each frame is the dmabuf of Ladybird's backing store, which Rune copies into a texture of
//! its own the first time it draws it, and then tells the engine it has read it. Ladybird paints
//! into the store again only after that.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::sync::{Mutex, OnceLock};

use elaris::ladybird::GpuFrame;
use rune::NativeSurface;

#[cfg(target_os = "macos")]
use rune::native_surface::macos::PixelBuffer;
#[cfg(target_os = "macos")]
use rune::native_surface::{ColorInfo, FrameTiming};

#[cfg(target_os = "macos")]
const CPU_FRAMES_VARIABLE: &str = "ELARIS_CPU_FRAMES";
#[cfg(target_os = "linux")]
const GPU_FRAMES_VARIABLE: &str = "ELARIS_GPU_FRAMES";

/// Whether new tabs show their frames through the GPU. On macOS that is the default and
/// `ELARIS_CPU_FRAMES=1` copies through the CPU instead; on Linux it is opt-in with
/// `ELARIS_GPU_FRAMES=1` while it is untested on real hardware, and only when Ladybird was built with
/// Vulkan dmabuf images.
pub(super) fn enabled() -> bool {
    #[cfg(target_os = "macos")]
    {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED
            .get_or_init(|| std::env::var_os(CPU_FRAMES_VARIABLE).is_none_or(|value| value == "0"))
    }
    #[cfg(target_os = "linux")]
    {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED
            .get_or_init(|| std::env::var_os(GPU_FRAMES_VARIABLE).is_some_and(|value| value != "0"))
    }
}

/// Whether the Compositor may show frames in a layer of its own (`Surface::hosts_layer`):
/// `ELARIS_REMOTE_LAYERS=1` turns it on and `CALayerHost` exists. Never in unit tests,
/// whose fake engine has no Compositor.
#[cfg(target_os = "macos")]
pub(super) fn remote_layers() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !cfg!(test)
            && std::env::var_os(REMOTE_LAYERS_VARIABLE).is_some_and(|value| value != "0")
            && remote_layer::supported()
    })
}

#[cfg(target_os = "macos")]
const REMOTE_LAYERS_VARIABLE: &str = "ELARIS_REMOTE_LAYERS";

#[cfg(target_os = "linux")]
type Wake = Arc<dyn Fn() + Send + Sync>;

/// Where the engine shows a tab's GPU frames, shared with the window that draws them.
#[derive(Clone)]
pub(crate) struct GpuSurface {
    native: NativeSurface,
    /// The newest frame serial Rune's GPU has finished with.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    completed: Arc<AtomicU64>,
    #[cfg(target_os = "linux")]
    linux: Arc<Linux>,
}

/// What the Linux path remembers between frames.
#[cfg(target_os = "linux")]
#[derive(Default)]
struct Linux {
    /// Wakes the engine thread when Rune has read a frame.
    wake: Mutex<Option<Wake>>,
    /// Each backing store's buffer as Rune imported it, which it does once per buffer.
    buffers: Mutex<std::collections::HashMap<BufferKey, Arc<DmabufBuffer>>>,
}

/// A backing store's dmabuf and the size of the frame shown from it.
#[cfg(target_os = "linux")]
type BufferKey = (i32, u64, u64, [u32; 2]);

#[cfg(target_os = "linux")]
use rune::native_surface::linux::{Dmabuf, DmabufBuffer, DmabufPlane};

/// A backing store is rarely more than a handful; this only bounds a resize's leftovers.
#[cfg(target_os = "linux")]
const KEPT_BUFFERS: usize = 32;

impl GpuSurface {
    pub(super) fn new() -> Self {
        Self {
            native: NativeSurface::new(),
            completed: Arc::new(AtomicU64::new(0)),
            #[cfg(target_os = "linux")]
            linux: Arc::default(),
        }
    }

    pub(crate) fn native(&self) -> &NativeSurface {
        &self.native
    }

    /// The newest frame serial Rune's GPU has finished with.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    pub(super) fn completed(&self) -> u64 {
        self.completed.load(Ordering::Acquire)
    }

    /// Runs `wake` whenever the GPU finishes a frame newer than any it finished before.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    pub(super) fn on_completed(&self, wake: impl Fn() + Send + Sync + 'static) {
        #[cfg(target_os = "macos")]
        {
            let completed = Arc::clone(&self.completed);
            self.native
                .set_observer(Arc::new(move |timing: FrameTiming| {
                    if timing.completed_at.is_some()
                        && completed.fetch_max(timing.frame_id, Ordering::AcqRel) < timing.frame_id
                    {
                        wake();
                    }
                }));
        }
        #[cfg(target_os = "linux")]
        {
            *self.linux.wake.lock().expect("GPU frame wake poisoned") = Some(Arc::new(wake));
        }
    }

    /// Shows the Compositor's CoreAnimation context `context_id` in place of pushed frames, or
    /// goes back to them for 0. Main thread only.
    #[cfg(target_os = "macos")]
    pub(crate) fn show_remote_layer(&self, context_id: u64) {
        let layer = u32::try_from(context_id)
            .ok()
            .filter(|&id| id != 0)
            .and_then(remote_layer::host);
        // SAFETY: `layer` is a CALayer made above, retained until the surface retains it.
        unsafe {
            self.native
                .set_layer(layer.as_ref().map(remote_layer::Host::as_ptr))
        };
    }

    /// Shows `frame` once Rune next draws, replacing any frame it has not drawn yet.
    #[cfg_attr(not(feature = "ladybird-bridge"), allow(dead_code))]
    pub(super) fn show(&self, frame: GpuFrame) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: The bridge finished its copy before returning the surface, and does not
            // write it again until Rune reports a frame newer than this one drawn.
            let buffer =
                unsafe { PixelBuffer::from_iosurface(frame.io_surface, ColorInfo::default())? };
            self.native.push(buffer, frame.serial)?;
        }
        #[cfg(target_os = "linux")]
        self.show_dmabuf(frame)?;
        // `Pixels::set` wakes the window for a new frame; pushing a surface frame does not, and
        // without it the page only redraws when something else, such as the pointer, does.
        super::wake_all_windows();
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn show_dmabuf(&self, frame: GpuFrame) -> Result<(), String> {
        use std::os::fd::OwnedFd;
        use std::os::unix::fs::MetadataExt;

        let GpuFrame {
            dmabuf,
            bitmap_id,
            width,
            height,
            serial,
            ..
        } = frame;
        // The descriptor names the buffer: stores keep theirs, so the inode tells a new buffer
        // from the one a retired store's number once had.
        let file = std::fs::File::from(dmabuf.fd);
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        let fd = OwnedFd::from(file);
        let key = (bitmap_id, metadata.dev(), metadata.ino(), [width, height]);
        let buffer = {
            let mut buffers = self.linux.buffers.lock().expect("GPU buffers poisoned");
            if buffers.len() >= KEPT_BUFFERS && !buffers.contains_key(&key) {
                buffers.clear();
            }
            match buffers.get(&key) {
                Some(buffer) => Arc::clone(buffer),
                None => {
                    let plane = DmabufPlane {
                        fd,
                        offset: dmabuf.offset,
                        stride: dmabuf.stride,
                        modifier: dmabuf.modifier,
                    };
                    let buffer =
                        Arc::new(DmabufBuffer::new(plane, [width, height], dmabuf.format)?);
                    buffers.insert(key, Arc::clone(&buffer));
                    buffer
                }
            }
        };
        let completed = Arc::clone(&self.completed);
        let linux = Arc::clone(&self.linux);
        let read = Dmabuf::new(buffer).on_read(move |result| {
            if let Err(error) = result {
                // The frame never shows, but the store must still go back to Ladybird.
                eprintln!("Elaris: a GPU frame could not be shown: {error}");
            }
            completed.fetch_max(serial, Ordering::AcqRel);
            if let Some(wake) = linux.wake.lock().expect("GPU frame wake poisoned").clone() {
                wake();
            }
        });
        self.native.push_dmabuf(read, serial)
    }
}

/// `CALayerHost` shows the layers another process commits into a CoreAnimation context. It is
/// not public API, so it is looked up and checked before use, as Ladybird's own macOS UI does.
#[cfg(target_os = "macos")]
mod remote_layer {
    use objc2_05::rc::Retained;
    use objc2_05::runtime::{AnyClass, AnyObject};
    use objc2_05::{msg_send, sel};
    use std::ptr::NonNull;

    fn class() -> Option<&'static AnyClass> {
        let class = AnyClass::get("CALayerHost")?;
        let layer = AnyClass::get("CALayer")?;
        let is_layer = std::iter::successors(Some(class), |class| class.superclass())
            .any(|ancestor| std::ptr::eq(ancestor, layer));
        (is_layer
            && class.instance_method(sel!(contextId)).is_some()
            && class.instance_method(sel!(setContextId:)).is_some())
        .then_some(class)
    }

    pub(super) fn supported() -> bool {
        class().is_some()
    }

    pub(super) struct Host(Retained<AnyObject>);

    impl Host {
        pub(super) fn as_ptr(&self) -> NonNull<std::ffi::c_void> {
            NonNull::from(&*self.0).cast()
        }
    }

    /// A layer showing context `context_id`.
    pub(super) fn host(context_id: u32) -> Option<Host> {
        // SAFETY: `class` checked that CALayerHost is a CALayer with a `contextId` property; the
        // new object is owned here.
        unsafe {
            let layer: *mut AnyObject = msg_send![class()?, new];
            let layer = Retained::from_raw(layer)?;
            let _: () = msg_send![&*layer, setContextId: context_id];
            Some(Host(layer))
        }
    }
}
