// SPDX-License-Identifier: GPL-3.0-only

//! Appearance preferences and their custom controls.

use rune::tok::type_;
use rune::{Drag, El, Phase, col, el, row, text};
use rune_kit::{Kit, SegmentedControl};

use crate::app::AppState;
use crate::appearance::{AccentColor, Appearance};
use crate::settings::TabLayout;
use crate::ui::theme::{self, Theme};

use super::{CONTROL_WIDTH, section, setting_row, settings_group};

const ACCENT_SWATCH_SIZE: f32 = 36.0;
const ACCENT_DOT_SIZE: f32 = 20.0;

pub(super) fn appearance_section(state: &AppState, theme: &Theme, kit: &Kit, compact: bool) -> El {
    let control = SegmentedControl::new(
        &["System", "Light", "Dark"],
        Some(match state.settings.appearance {
            Appearance::System => 0,
            Appearance::Light => 1,
            Appearance::Dark => 2,
        }),
    )
    .width(Some(CONTROL_WIDTH))
    .label("Appearance")
    .on_pick(|state: &mut AppState, index, _| {
        let appearance = match index {
            0 => Appearance::System,
            1 => Appearance::Light,
            _ => Appearance::Dark,
        };
        state.set_appearance(appearance);
    })
    .build(kit);

    let tab_layout = SegmentedControl::new(
        &TabLayout::ALL.map(TabLayout::label),
        TabLayout::ALL
            .iter()
            .position(|layout| *layout == state.settings.tab_layout),
    )
    .width(Some(CONTROL_WIDTH))
    .label("Tab Layout")
    .on_pick(|state: &mut AppState, index, tx| {
        state.set_tab_layout(TabLayout::ALL[index]);
        tx.animated();
        tx.motion(theme::SIDEBAR_TOGGLE);
    })
    .build(kit);

    // Rune Kit's standard switch uses the strong icon token for its checked track.
    let mut switch_kit = kit.clone();
    switch_kit.c.std_icon_strong = kit.c.bg_accent;
    let glass = rune_kit::Switch::new(state.settings.glass_enabled)
        .standard()
        .label("Liquid glass background")
        .on_toggle(|state: &mut AppState| state.set_glass_enabled(!state.settings.glass_enabled))
        .build(&switch_kit);
    let opacity = tint_opacity_control(state.settings.tint_opacity, theme);
    section(
        "Appearance",
        theme,
        settings_group(
            col()
                .gap(12.0)
                .child(setting_row(
                    "Theme",
                    "Choose how Elaris and webpages look.",
                    theme,
                    control,
                    compact,
                ))
                .child(setting_row(
                    "Tab Layout",
                    "Choose where open tabs appear.",
                    theme,
                    tab_layout,
                    compact,
                ))
                .child(setting_row(
                    "Accent colour",
                    "Choose the colour used for interactive emphasis.",
                    theme,
                    accent_color_control(state.settings.accent_color, theme),
                    compact,
                ))
                .child(setting_row(
                    "Liquid glass",
                    "Show the tinted glass window background.",
                    theme,
                    glass,
                    compact,
                ))
                .child(setting_row(
                    "Tint opacity",
                    "100% keeps the current opaque tint.",
                    theme,
                    opacity,
                    compact,
                )),
            theme,
        ),
    )
}

fn accent_color_control(selected: AccentColor, theme: &Theme) -> El {
    let mut choices = row()
        .key("accent-colour-options")
        .role("radiogroup")
        .label("Accent colour")
        .w(ACCENT_SWATCH_SIZE * AccentColor::ALL.len() as f32 + 5.0 * 5.0)
        .gap(5.0)
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
    const WIDTH: f32 = 220.0;
    const KNOB: f32 = 16.0;
    let value = value.min(100);
    let fraction = f32::from(value) / 100.0;
    let travel = WIDTH - KNOB;
    let position = KNOB / 2.0 + fraction * travel;
    row()
        .gap(8.0)
        .center_y()
        .child(
            el().key("tint-opacity-slider")
                .role("slider")
                .label("Tint opacity")
                .range_value(f64::from(value), 0.0, 100.0)
                .range_step(1.0)
                .a11y_value(format!("{value}%"))
                .size(WIDTH, 22.0)
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
                        .absolute(0.0, 9.0)
                        .size(WIDTH, 4.0)
                        .radius(2.0)
                        .fill(theme.control_border),
                )
                .child(
                    el().key("fill")
                        .absolute(0.0, 9.0)
                        .size(position, 4.0)
                        .radius(2.0)
                        .fill(theme.focus),
                )
                .child(
                    el().key("knob")
                        .absolute(position - KNOB / 2.0, 3.0)
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
