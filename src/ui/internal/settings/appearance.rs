// SPDX-License-Identifier: GPL-3.0-only

//! Look & layout: theme, tabs, accent, glass, Split View and motion.

use rune::tok::type_;
use rune::{Drag, El, Phase, el, row, text};
use rune_kit::SegmentedControl;

use crate::app::AppState;
use crate::appearance::{AccentColor, Appearance};
use crate::settings::{NewTabPosition, SidebarSide, TabLayout};
use crate::ui::internal::PageContext;
use crate::ui::internal::widgets::{setting, switch_kit};
use crate::ui::theme::{self, Theme};

use super::section;

const ACCENT_SWATCH_SIZE: f32 = 36.0;
const ACCENT_DOT_SIZE: f32 = 20.0;
const ACCENT_GAP: f32 = 5.0;
const SLIDER_HEIGHT: f32 = 22.0;
const SLIDER_TRACK: f32 = 4.0;
const SLIDER_KNOB: f32 = 16.0;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    vec![appearance(cx), split_view(cx), motion(cx)]
}

fn appearance(cx: &mut PageContext) -> El {
    let state = cx.state;
    let theme_control = SegmentedControl::new(
        &["System", "Light", "Dark"],
        Some(match state.settings.appearance {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        }),
    )
    .width(Some(theme::PAGE_CONTROL_WIDTH))
    .label("Appearance")
    .on_pick(|state: &mut AppState, index, _| {
        let appearance = match index {
            0 => Appearance::System,
            1 => Appearance::Light,
            _ => Appearance::Dark,
        };
        state.set_appearance(appearance);
    })
    .build(&cx.kit);
    let tab_layout = SegmentedControl::new(
        &TabLayout::ALL.map(TabLayout::label),
        TabLayout::ALL
            .iter()
            .position(|layout| *layout == state.settings.tab_layout),
    )
    .width(Some(theme::PAGE_CONTROL_WIDTH))
    .label("Tab Layout")
    .on_pick(|state: &mut AppState, index, tx| {
        state.set_tab_layout(TabLayout::ALL[index]);
        tx.animated();
        tx.motion(theme::SIDEBAR_TOGGLE);
    })
    .build(&cx.kit);
    let sidebar_side = SegmentedControl::new(
        &SidebarSide::ALL.map(SidebarSide::label),
        SidebarSide::ALL
            .iter()
            .position(|side| *side == state.settings.sidebar_side),
    )
    .width(Some(theme::PAGE_CONTROL_WIDTH))
    .label("Sidebar Position")
    .on_pick(|state: &mut AppState, index, tx| {
        state.set_sidebar_side(SidebarSide::ALL[index]);
        tx.animated();
        tx.motion(theme::SIDEBAR_TOGGLE);
    })
    .build(&cx.kit);
    let new_tab_position = SegmentedControl::new(
        &NewTabPosition::ALL.map(NewTabPosition::label),
        NewTabPosition::ALL
            .iter()
            .position(|position| *position == state.settings.new_tab_position),
    )
    .width(Some(theme::PAGE_CONTROL_WIDTH))
    .label("New Tab Button")
    .on_pick(|state: &mut AppState, index, tx| {
        state.set_new_tab_position(NewTabPosition::ALL[index]);
        tx.animated();
        tx.motion(theme::TAB_REORDER_MOTION);
    })
    .build(&cx.kit);
    let mut rows = vec![
        setting(
            cx,
            "Theme",
            "Choose how Elaris and webpages look.",
            theme_control,
        ),
        setting(
            cx,
            "Tab Layout",
            "Choose where open tabs appear.",
            tab_layout,
        ),
        setting(
            cx,
            "Sidebar position",
            "Choose which side of the window the vertical sidebar sits on.",
            sidebar_side,
        ),
        setting(
            cx,
            "New Tab button",
            "Choose whether New Tab sits above or below the tabs in the vertical sidebar.",
            new_tab_position,
        ),
        setting(
            cx,
            "Accent colour",
            "Choose the colour used for interactive emphasis.",
            accent_color_control(state.settings.accent_color, cx.theme),
        ),
    ];
    // The glass material is macOS only, so Linux offers neither glass option.
    if !cfg!(elaris_ui = "linux") {
        let glass = rune_kit::Switch::new(state.settings.glass_enabled)
            .standard()
            .label("Liquid glass background")
            .hide_label()
            .on_toggle_tx(|state: &mut AppState, tx| {
                state.set_glass_enabled(!state.settings.glass_enabled);
                tx.animated();
            })
            .build(&switch_kit(&cx.kit));
        rows.push(setting(
            cx,
            "Liquid glass",
            "Show the tinted glass window background.",
            glass,
        ));
        // The tint only shows through glass, so its row leaves while glass is off.
        if state.settings.glass_enabled {
            rows.push(setting(
                cx,
                "Tint opacity",
                "100% keeps the current opaque tint.",
                tint_opacity_control(state.settings.tint_opacity, cx.theme),
            ));
        }
    }
    section("Appearance", cx.theme, rows)
}

fn split_view(cx: &mut PageContext) -> El {
    const OPTIONS: [usize; 3] = [2, 3, 4];
    let selected = OPTIONS
        .iter()
        .position(|limit| *limit == cx.state.settings.split_view_limit.min(4));
    let control = SegmentedControl::new(&["2", "3", "4"], selected)
        .width(Some(theme::PAGE_CONTROL_WIDTH))
        .label("Maximum split panes")
        .on_pick(|state: &mut AppState, index, _| state.set_split_view_limit(OPTIONS[index]))
        .build(&cx.kit);
    section(
        "Split View",
        cx.theme,
        [setting(
            cx,
            "Maximum panes",
            "Choose how many pages can appear in the split view grid. Higher limits are available in config.toml.",
            control,
        )],
    )
}

fn motion(cx: &mut PageContext) -> El {
    let reduce_motion = rune_kit::Switch::new(cx.state.settings.reduce_motion)
        .standard()
        .label("Reduce motion")
        .hide_label()
        .on_toggle(|state: &mut AppState| state.set_reduce_motion(!state.settings.reduce_motion))
        .build(&switch_kit(&cx.kit));
    section(
        "Performance",
        cx.theme,
        [setting(
            cx,
            "Reduce motion",
            "Minimise animations and transitions throughout Elaris.",
            reduce_motion,
        )],
    )
}

fn accent_color_control(selected: AccentColor, theme: &Theme) -> El {
    let mut choices = row()
        .key("accent-colour-options")
        .role("radiogroup")
        .label("Accent colour")
        .w(ACCENT_SWATCH_SIZE * AccentColor::ALL.len() as f32
            + ACCENT_GAP * (AccentColor::ALL.len() - 1) as f32)
        .gap(ACCENT_GAP)
        .center_y();

    for (index, accent) in AccentColor::ALL.into_iter().enumerate() {
        let is_selected = selected == accent;
        let mut choice = el()
            .key(format!("accent-colour-{}", accent.config_key()))
            .role("radio")
            .label(accent.label())
            .checked(is_selected)
            .position_in_set(index + 1, AccentColor::ALL.len())
            .size(ACCENT_SWATCH_SIZE, ACCENT_SWATCH_SIZE)
            .radius(ACCENT_SWATCH_SIZE / 2.0)
            .center()
            .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
            .hover(theme.hover)
            .press(theme.press)
            .on_press(move |state: &mut AppState| state.set_accent_color(accent))
            .child(
                el().key("swatch")
                    .size(ACCENT_DOT_SIZE, ACCENT_DOT_SIZE)
                    .radius(ACCENT_DOT_SIZE / 2.0)
                    .fill(theme::accent_color(accent))
                    .a11y_hidden(),
            );
        if is_selected {
            choice = choice.border(3.0, theme.accent);
        }
        choices = choices.child(choice);
    }

    choices
}

fn tint_opacity_control(value: u8, theme: &Theme) -> El {
    const WIDTH: f32 = theme::PAGE_CONTROL_WIDTH;
    const KNOB: f32 = SLIDER_KNOB;
    let value = value.min(100);
    let fraction = f32::from(value) / 100.0;
    let travel = WIDTH - KNOB;
    let position = KNOB / 2.0 + fraction * travel;
    row()
        .gap(rune::tok::space::S2)
        .center_y()
        .child(
            el().key("tint-opacity-slider")
                .role("slider")
                .label("Tint opacity")
                .range_value(f64::from(value), 0.0, 100.0)
                .range_step(1.0)
                .a11y_value(format!("{value}%"))
                .size(WIDTH, SLIDER_HEIGHT)
                .on_step(|state: &mut AppState, steps, _| {
                    state.set_tint_opacity(
                        (i16::from(state.settings.tint_opacity) + steps as i16).clamp(0, 100) as u8,
                    );
                })
                .on_drag(
                    move |state: &mut AppState, drag: &mut Drag, _| match drag.phase {
                        Phase::Down => {
                            drag.data = f32::from(state.settings.tint_opacity);
                            state.set_tint_opacity(
                                (((drag.x - KNOB / 2.0) / travel).clamp(0.0, 1.0) * 100.0).round()
                                    as u8,
                            );
                        }
                        Phase::Move => {
                            state.set_tint_opacity(
                                (((drag.x - KNOB / 2.0) / travel).clamp(0.0, 1.0) * 100.0).round()
                                    as u8,
                            );
                        }
                        Phase::Up if drag.cancelled() => {
                            state.set_tint_opacity(drag.data.clamp(0.0, 100.0) as u8)
                        }
                        Phase::Up => {}
                    },
                )
                .child(
                    el().key("track")
                        .absolute(0.0, (SLIDER_HEIGHT - SLIDER_TRACK) / 2.0)
                        .size(WIDTH, SLIDER_TRACK)
                        .radius(SLIDER_TRACK / 2.0)
                        .fill(theme.control_border),
                )
                .child(
                    el().key("fill")
                        .absolute(0.0, (SLIDER_HEIGHT - SLIDER_TRACK) / 2.0)
                        .size(position, SLIDER_TRACK)
                        .radius(SLIDER_TRACK / 2.0)
                        .fill(theme.focus),
                )
                .child(
                    el().key("knob")
                        .absolute(position - KNOB / 2.0, (SLIDER_HEIGHT - KNOB) / 2.0)
                        .size(KNOB, KNOB)
                        .radius(KNOB / 2.0)
                        .fill(theme.icon),
                ),
        )
        .child(
            text(format!("{value}%"))
                .font(type_::LABEL)
                .color(theme.text),
        )
}
