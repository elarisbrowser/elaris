// SPDX-License-Identifier: GPL-3.0-only

use crate::ui::components::animated_icon::{self, AnimatedIcon};
use crate::ui::components::menu::{Menu, MenuItem, context_menu};
use crate::ui::icons;
use crate::ui::tab_switcher::{tab_labels, tab_symbol};
use crate::ui::theme;
use elaris::ladybird::{Edit, MediaAction, PageMenu};
use rune::tok::type_;
use rune::{El, PointerPress, Tx, col, el, row, text};

use crate::{
    app::AppState,
    engine::ViewCommand,
    tab::{Tab, TabId},
    ui::theme::Theme,
};

/// The menu's key, which the native page menu shares for the same highlight and dismissal.
pub(crate) const KEY: &str = "page-context-menu";

#[derive(Default)]
pub(crate) struct PageMenuState {
    pub(super) anchor: Option<(TabId, PointerPress)>,
    current: Option<(TabId, PointerPress, PageMenu)>,
    /// A menu on a native page, which Elaris builds without asking the engine.
    native: Option<(TabId, PointerPress)>,
}
impl PageMenuState {
    /// Opens the menu the engine built for tab `id`, if it answers the pending press.
    pub(crate) fn receive(&mut self, id: TabId, menu: PageMenu) -> bool {
        let Some((owner, point)) = self.anchor.take().filter(|(owner, _)| *owner == id) else {
            return false;
        };
        self.current = Some((owner, point, menu));
        true
    }
    pub(crate) fn open(&self) -> bool {
        self.current.is_some() || self.native.is_some()
    }

    pub(super) fn open_native(&mut self, id: TabId, point: PointerPress) {
        *self = Self {
            native: Some((id, point)),
            ..Self::default()
        };
    }
}

/// Runs `action` while tab `id` is still the active one, returns focus to the page, then closes
/// the menu.
fn act(
    id: TabId,
    action: impl Fn(&mut AppState, &mut Tx) + 'static,
) -> impl Fn(&mut AppState, &mut Tx) + 'static {
    move |s, tx| {
        if s.browser.active_tab().is_some_and(|tab| tab.id() == id) && !s.omnibox.open {
            tx.focus(crate::ui::viewport::focus_key(&s.browser), false);
            action(s, tx);
        }
        s.page_menu = Default::default();
        tx.animated();
    }
}

fn item(id: TabId, label: &str, action: impl Fn(&mut AppState, &mut Tx) + 'static) -> MenuItem {
    MenuItem::new(label).on_select_tx(act(id, action))
}

/// Adds the page's mark, title and host above `menu`'s rows, and Back, Forward and Reload
/// buttons leading them.
pub(crate) fn with_header(menu: Menu, state: &AppState, tab: &Tab, theme: &Theme) -> Menu {
    menu.header(identity(tab, theme), navigation(state, tab, theme))
}

fn identity(tab: &Tab, theme: &Theme) -> El {
    let (title, address) = tab_labels(tab);
    let mut labels = col().min_w(0.0).grow(1.0).child(
        text(title)
            .font(theme::MENU_HEADER_TITLE)
            .color(theme.text)
            .single_line()
            .truncate(),
    );
    if !address.is_empty() && address != title {
        labels = labels.child(
            text(address)
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line()
                .truncate(),
        );
    }
    row()
        .key(format!("{KEY}-identity"))
        .padding(theme::MENU_HEADER_PADDING)
        .gap(theme::MENU_ROW_PADDING)
        .center_y()
        .a11y_hidden()
        .child(
            el().size(theme::MENU_HEADER_TILE, theme::MENU_HEADER_TILE)
                .center()
                .radius(theme::MENU_ITEM_RADIUS)
                .continuous_corners()
                .fill(theme.overlay_control)
                .child(tab_symbol(tab, theme::MENU_HEADER_ICON, theme)),
        )
        .child(labels)
}

fn navigation(state: &AppState, tab: &Tab, theme: &Theme) -> El {
    let id = tab.id();
    row()
        .key(format!("{KEY}-navigation"))
        .gap(theme::MENU_NAV_GAP)
        .pb(theme::MENU_NAV_GAP)
        .child(nav_button(
            state,
            "Back",
            &animated_icon::BACK,
            tab.can_go_back,
            theme,
            act(id, move |s, _| s.browser.go_back(id)),
        ))
        .child(nav_button(
            state,
            "Forward",
            &animated_icon::FORWARD,
            tab.can_go_forward,
            theme,
            act(id, move |s, _| s.browser.go_forward(id)),
        ))
        .child(nav_button(
            state,
            "Reload",
            &animated_icon::RELOAD,
            true,
            theme,
            act(id, move |s, _| s.browser.reload(id)),
        ))
}

/// One of the header's equal-width buttons. A disabled one keeps its fill and dims its content.
fn nav_button(
    state: &AppState,
    label: &'static str,
    glyph: &'static AnimatedIcon,
    enabled: bool,
    theme: &Theme,
    press: impl Fn(&mut AppState, &mut Tx) + 'static,
) -> El {
    let reduced = state.settings.reduce_motion;
    let owner = format!("{KEY}-nav-{label}");
    let content = if enabled { 1.0 } else { theme.disabled_opacity };
    let button = row()
        .key(owner.clone())
        .role("button")
        .label(label)
        .w(theme::MENU_NAV_WIDTH)
        .h(theme::MENU_NAV_HEIGHT)
        .gap(theme::MENU_ICON_GAP)
        .center()
        .radius(theme::MENU_NAV_RADIUS)
        .continuous_corners()
        .fill(theme.overlay_control)
        .disabled(!enabled)
        .focusable(enabled)
        .child(
            animated_icon::glyph(
                state,
                glyph,
                &owner,
                &format!("{owner}-glyph"),
                theme::MENU_ICON,
                theme.icon,
                reduced,
            )
            .opacity(content)
            .a11y_hidden(),
        )
        .child(
            text(label)
                .font(type_::BODY)
                .color(theme.text)
                .opacity(content)
                .single_line()
                .a11y_hidden(),
        );
    if !enabled {
        return button;
    }
    button
        .hover(theme.overlay_control_hover)
        .focus_highlight(theme.overlay_control_hover)
        .on_hover_change(animated_icon::hover_change(owner))
        .on_press_tx(press)
}

pub(crate) fn view(state: &AppState, theme: &Theme, layer: &mut Vec<El>) {
    if let Some((id, point)) = state.page_menu.native {
        crate::ui::internal::context_menu(state, id, point, theme, layer);
        return;
    }
    let Some((id, point, info)) = &state.page_menu.current else {
        return;
    };
    let Some(tab) = state.browser.active_tab().filter(|tab| tab.id() == *id) else {
        return;
    };
    let id = *id;
    let request = info.request;
    let mut menu = context_menu(KEY, el(), Some(*point))
        .on_close(|s: &mut AppState| s.page_menu = Default::default());
    if let Some(media) = info.media {
        menu = menu.item(
            item(
                id,
                if media.playing { "Pause" } else { "Play" },
                move |s, _| {
                    s.browser
                        .send(id, ViewCommand::MediaAction(MediaAction::TogglePlay))
                },
            )
            .icon(if media.playing {
                &icons::PAUSE
            } else {
                &icons::RESUME
            }),
        );
        menu = menu.item(item(
            id,
            if media.muted { "Unmute" } else { "Mute" },
            move |s, _| {
                s.browser
                    .send(id, ViewCommand::MediaAction(MediaAction::ToggleMute))
            },
        ));
        if media.can_enter_picture_in_picture || media.in_picture_in_picture {
            let label = if media.in_picture_in_picture {
                "Exit Picture in Picture"
            } else {
                "Picture in Picture"
            };
            menu = menu.item(item(id, label, move |s, _| {
                s.browser.send(
                    id,
                    ViewCommand::MediaAction(MediaAction::TogglePictureInPicture),
                )
            }));
        }
    }
    if !info.url.is_empty() {
        if !info.image {
            menu = menu.item(
                item(id, "Open Link", move |s, _| {
                    s.browser.send(
                        id,
                        ViewCommand::OpenContext {
                            request,
                            new_tab: false,
                        },
                    )
                })
                .icon(&icons::OPEN),
            );
        }
        menu = menu.item(
            item(
                id,
                if info.image {
                    "Open Image in New Tab"
                } else {
                    "Open Link in New Tab"
                },
                move |s, _| {
                    s.browser.send(
                        id,
                        ViewCommand::OpenContext {
                            request,
                            new_tab: true,
                        },
                    )
                },
            )
            .icon(&icons::OPEN_TAB),
        );
        let url = info.url.clone();
        menu = menu.item(
            item(
                id,
                if info.image {
                    "Copy Image Address"
                } else {
                    "Copy Link Address"
                },
                move |_, tx| tx.copy(&url),
            )
            .icon(&icons::LINK),
        );
    } else if info.media.is_none() {
        menu = with_header(menu, state, tab, theme).item(item(
            id,
            "Reload Without Cache",
            move |s, _| s.browser.reload_bypassing_cache(id),
        ));
    }
    if !info.selection.is_empty() {
        let text = info.selection.clone();
        menu =
            menu.item(item(id, "Copy", move |_, tx| tx.copy(&text)).animated(&animated_icon::COPY));
        if info.editable {
            menu = menu.item(item(id, "Cut", move |s, _| {
                s.browser.send(id, ViewCommand::Edit(Edit::Cut))
            }));
        }
    }
    if info.editable {
        menu = menu.item(item(id, "Paste", move |_, tx| super::paste(id, tx)));
    }
    if info.url.is_empty() {
        menu = menu.item(item(id, "Select All", move |s, _| {
            s.browser.send(id, ViewCommand::Edit(Edit::SelectAll))
        }));
    }
    menu.build(state, theme, layer);
}
