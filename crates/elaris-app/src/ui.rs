use rune::tok::{self, type_};
use rune::{Cx, El, col, el, row, text_input};

use crate::app_state::AppState;

pub(crate) fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let theme = tok::theme(true);
    let address = text_input(&state.address, type_::BODY)
        .key("address")
        .single_line()
        .w_full()
        .h(40.0)
        .px(12.0)
        .color(theme.text)
        .placeholder("Search or enter address", theme.text_secondary)
        .label("Address or search")
        .on_edit(|state: &mut AppState| &mut state.address);

    let toolbar = row()
        .key("toolbar")
        .w_full()
        .h(64.0)
        .pl(16.0 + cx.titlebar_inset())
        .pr(16.0)
        .center_y()
        .child(
            el().key("address-container")
                .w_full()
                .h(40.0)
                .px(4.0)
                .center_y()
                .radius(10.0)
                .fill(theme.control_off)
                .child(address),
        );

    let content = el().key("content").w_full().grow(1.0).fill(theme.bg);

    col()
        .key("window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(toolbar)
        .child(content)
}
