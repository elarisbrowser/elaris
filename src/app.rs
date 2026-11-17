// SPDX-License-Identifier: GPL-3.0-only

use rune::TextEdit;

#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) address: TextEdit,
}
