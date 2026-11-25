// SPDX-License-Identifier: GPL-3.0-only

//! The DRM device the Wayland compositor presents on, as it names it in its default dmabuf
//! feedback (`UI/Qt/WaylandGpu.cpp` in Ladybird). Ladybird's Compositor paints there too, so the
//! desktop's compositor can show its frames without a copy from another GPU, and a hybrid laptop's
//! other GPU can sleep.

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::{
    self, ZwpLinuxDmabufFeedbackV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;

/// The compositor's main device (`dev_t`), on a connection of its own. None off Wayland, and when
/// the compositor sends no dmabuf feedback (before version 4).
pub(crate) fn main_device() -> Option<u64> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Query>(&conn).ok()?;
    let dmabuf = globals
        .bind::<ZwpLinuxDmabufV1, _, _>(&queue.handle(), 4..=4, ())
        .ok()?;
    let feedback = dmabuf.get_default_feedback(&queue.handle(), ());
    let mut query = Query::default();
    // The compositor sends the whole feedback as soon as it is asked for.
    queue.roundtrip(&mut query).ok()?;
    feedback.destroy();
    dmabuf.destroy();
    let _ = conn.flush();
    query.main_device
}

#[derive(Default)]
struct Query {
    main_device: Option<u64>,
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for Query {
    fn event(
        query: &mut Self,
        _: &ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } = event {
            query.main_device = device.try_into().ok().map(u64::from_ne_bytes);
        }
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for Query {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxDmabufV1,
        _: <ZwpLinuxDmabufV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Query {
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
