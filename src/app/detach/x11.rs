// SPDX-License-Identifier: GPL-3.0-only

use rune::{WindowHandle, raw_window_handle::RawWindowHandle};
use x11rb::{
    connection::Connection,
    protocol::{
        randr::ConnectionExt as _,
        xproto::{Atom, AtomEnum, ConnectionExt as _, Window},
    },
};

#[derive(Clone, Copy)]
pub(super) struct Placement {
    pointer: (i32, i32),
    bounds: [i32; 4],
}

impl Placement {
    pub(super) fn apply(self, window: &WindowHandle) {
        let Some(window_placement) = window.placement() else {
            return;
        };
        let [left, top, width, height] = self.bounds;
        let w = (window_placement.content_size.0 as i32).min(width);
        let h = (window_placement.content_size.1 as i32).min(height);
        let x = (self.pointer.0 - w / 4).clamp(left, left + width - w);
        let y =
            (self.pointer.1 - (48.0 * window_placement.scale) as i32).clamp(top, top + height - h);
        if (w as u32, h as u32) != window_placement.content_size {
            window.resize_instant((
                w as f32 / window_placement.scale as f32,
                h as f32 / window_placement.scale as f32,
            ));
        }
        window.set_position((x, y));
    }
}

pub(super) fn placement(window: &WindowHandle) -> Option<Placement> {
    window
        .with_raw_handle(|handle| {
            matches!(
                handle.as_raw(),
                RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)
            )
        })
        .ok()?
        .then_some(())?;
    let source = window.placement()?;
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let pointer = connection.query_pointer(root).ok()?.reply().ok()?;
    if !pointer.same_screen || (pointer.child != 0 && !desktop_window(&connection, pointer.child)) {
        return None;
    }
    let (x, y) = (i32::from(pointer.root_x), i32::from(pointer.root_y));
    let margin = (28.0 * source.scale) as i32;
    let (left, top) = source.position;
    if x >= left - margin
        && x < source.content_position.0 + source.content_size.0 as i32 + margin
        && y >= top - margin
        && y < source.content_position.1 + source.content_size.1 as i32 + margin
    {
        return None;
    }
    let bounds = connection
        .randr_get_monitors(root, true)
        .ok()
        .and_then(|reply| reply.reply().ok())
        .and_then(|reply| {
            reply
                .monitors
                .into_iter()
                .map(|monitor| {
                    [
                        i32::from(monitor.x),
                        i32::from(monitor.y),
                        i32::from(monitor.width),
                        i32::from(monitor.height),
                    ]
                })
                .find(|bounds| contains(*bounds, (x, y)))
        })
        .or_else(|| {
            let ((left, top), (width, height)) = source.display?;
            let bounds = [left, top, width as i32, height as i32];
            contains(bounds, (x, y)).then_some(bounds)
        })?;
    let bounds = work_area(&connection, root)
        .and_then(|work_area| intersection(bounds, work_area))
        .unwrap_or(bounds);
    contains(bounds, (x, y)).then_some(Placement {
        pointer: (x, y),
        bounds,
    })
}

fn atom(connection: &impl Connection, name: &[u8]) -> Option<Atom> {
    let atom = connection.intern_atom(true, name).ok()?.reply().ok()?.atom;
    (atom != 0).then_some(atom)
}

fn desktop_window(connection: &impl Connection, window: Window) -> bool {
    let Some(kind) = atom(connection, b"_NET_WM_WINDOW_TYPE") else {
        return false;
    };
    let Some(desktop) = atom(connection, b"_NET_WM_WINDOW_TYPE_DESKTOP") else {
        return false;
    };
    connection
        .get_property(false, window, kind, AtomEnum::ATOM, 0, 16)
        .ok()
        .and_then(|reply| reply.reply().ok())
        .is_some_and(|reply| {
            reply
                .value32()
                .is_some_and(|mut values| values.any(|value| value == desktop))
        })
}

fn work_area(connection: &impl Connection, root: Window) -> Option<[i32; 4]> {
    let current = atom(connection, b"_NET_CURRENT_DESKTOP")?;
    let work_area = atom(connection, b"_NET_WORKAREA")?;
    let current = connection
        .get_property(false, root, current, AtomEnum::CARDINAL, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()?;
    let reply = connection
        .get_property(
            false,
            root,
            work_area,
            AtomEnum::CARDINAL,
            current.checked_mul(4)?,
            4,
        )
        .ok()?
        .reply()
        .ok()?;
    let mut values = reply.value32()?;
    Some([
        values.next()? as i32,
        values.next()? as i32,
        i32::try_from(values.next()?).ok()?,
        i32::try_from(values.next()?).ok()?,
    ])
}

fn contains([left, top, width, height]: [i32; 4], (x, y): (i32, i32)) -> bool {
    x >= left && x < left.saturating_add(width) && y >= top && y < top.saturating_add(height)
}

fn intersection([x, y, w, h]: [i32; 4], [a, b, c, d]: [i32; 4]) -> Option<[i32; 4]> {
    let left = x.max(a);
    let top = y.max(b);
    let right = x.saturating_add(w).min(a.saturating_add(c));
    let bottom = y.saturating_add(h).min(b.saturating_add(d));
    (right > left && bottom > top).then(|| [left, top, right - left, bottom - top])
}
