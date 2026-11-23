// SPDX-License-Identifier: GPL-3.0-only

//! Menu item specifications, and the tags that link native menu items back to their actions.

use crate::shortcuts::Shortcut;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Shortcut(Shortcut),
    Back,
    Forward,
    Favourite,
    SplitLeft,
    SplitRight,
    ExitSplit,
    Edit(EditAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

#[derive(Clone, Copy)]
pub(super) struct ItemSpec {
    pub(super) title: &'static str,
    pub(super) action: Option<Action>,
    pub(super) key: &'static str,
    pub(super) shift: bool,
    pub(super) option: bool,
    pub(super) control: bool,
}

pub(super) const fn item(
    title: &'static str,
    action: Action,
    key: &'static str,
    shift: bool,
) -> ItemSpec {
    ItemSpec {
        title,
        action: Some(action),
        key,
        shift,
        option: false,
        control: false,
    }
}

pub(super) const fn edit(
    title: &'static str,
    action: EditAction,
    key: &'static str,
    shift: bool,
) -> ItemSpec {
    item(title, Action::Edit(action), key, shift)
}

pub(super) const fn native(title: &'static str, key: &'static str, shift: bool) -> ItemSpec {
    ItemSpec {
        title,
        action: None,
        key,
        shift,
        option: false,
        control: false,
    }
}

pub(super) const fn native_option(title: &'static str, key: &'static str) -> ItemSpec {
    ItemSpec {
        title,
        action: None,
        key,
        shift: false,
        option: true,
        control: false,
    }
}

pub(super) const fn native_control(title: &'static str, key: &'static str) -> ItemSpec {
    ItemSpec {
        title,
        action: None,
        key,
        shift: false,
        option: false,
        control: true,
    }
}

pub(super) const fn separator() -> ItemSpec {
    ItemSpec {
        title: "",
        action: None,
        key: "",
        shift: false,
        option: false,
        control: false,
    }
}

pub(super) fn action_id(action: Action) -> isize {
    match action {
        Action::Shortcut(shortcut) => 100 + shortcut_id(shortcut),
        Action::Back => 1,
        Action::Forward => 2,
        Action::Favourite => 4,
        Action::SplitLeft => 5,
        Action::SplitRight => 6,
        Action::ExitSplit => 7,
        Action::Edit(edit) => {
            8 + match edit {
                EditAction::Undo => 0,
                EditAction::Redo => 1,
                EditAction::Cut => 2,
                EditAction::Copy => 3,
                EditAction::Paste => 4,
                EditAction::SelectAll => 5,
            }
        }
    }
}

pub(super) fn action_from_id(id: isize) -> Option<Action> {
    Some(match id {
        1 => Action::Back,
        2 => Action::Forward,
        4 => Action::Favourite,
        5 => Action::SplitLeft,
        6 => Action::SplitRight,
        7 => Action::ExitSplit,
        8 => Action::Edit(EditAction::Undo),
        9 => Action::Edit(EditAction::Redo),
        10 => Action::Edit(EditAction::Cut),
        11 => Action::Edit(EditAction::Copy),
        12 => Action::Edit(EditAction::Paste),
        13 => Action::Edit(EditAction::SelectAll),
        100 => Action::Shortcut(Shortcut::NewTab),
        101 => Action::Shortcut(Shortcut::Close),
        102 => Action::Shortcut(Shortcut::ReopenClosedTab),
        103 => Action::Shortcut(Shortcut::Find),
        104 => Action::Shortcut(Shortcut::Reload),
        105 => Action::Shortcut(Shortcut::ToggleSidebar),
        106 => Action::Shortcut(Shortcut::OpenHistory),
        107 => Action::Shortcut(Shortcut::NextTab),
        108 => Action::Shortcut(Shortcut::PreviousTab),
        109 => Action::Shortcut(Shortcut::OpenSettings),
        110 => Action::Shortcut(Shortcut::OpenFile),
        111 => Action::Shortcut(Shortcut::ReloadBypassingCache),
        112 => Action::Shortcut(Shortcut::ZoomIn),
        113 => Action::Shortcut(Shortcut::ZoomOut),
        114 => Action::Shortcut(Shortcut::ResetZoom),
        115 => Action::Shortcut(Shortcut::NewPrivateWindow),
        116 => Action::Shortcut(Shortcut::OpenTaskManager),
        117 => Action::Shortcut(Shortcut::CopyAddress),
        118 => Action::Shortcut(Shortcut::BookmarkPage),
        119 => Action::Shortcut(Shortcut::OpenBookmarks),
        120 => Action::Shortcut(Shortcut::BookmarkAllTabs),
        _ => return None,
    })
}

fn shortcut_id(shortcut: Shortcut) -> isize {
    match shortcut {
        Shortcut::NewTab => 0,
        Shortcut::Close => 1,
        Shortcut::ReopenClosedTab => 2,
        Shortcut::Find => 3,
        Shortcut::Reload => 4,
        Shortcut::ToggleSidebar => 5,
        Shortcut::OpenHistory => 6,
        Shortcut::NextTab => 7,
        Shortcut::PreviousTab => 8,
        Shortcut::OpenSettings => 9,
        Shortcut::OpenFile => 10,
        Shortcut::ReloadBypassingCache => 11,
        Shortcut::ZoomIn => 12,
        Shortcut::ZoomOut => 13,
        Shortcut::ResetZoom => 14,
        Shortcut::NewPrivateWindow => 15,
        Shortcut::OpenTaskManager => 16,
        Shortcut::CopyAddress => 17,
        Shortcut::BookmarkPage => 18,
        Shortcut::OpenBookmarks => 19,
        Shortcut::BookmarkAllTabs => 20,
        _ => -1,
    }
}
