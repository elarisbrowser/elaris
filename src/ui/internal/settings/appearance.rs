// SPDX-License-Identifier: GPL-3.0-only

//! Look & layout: theme, tabs, accent, glass, Split View, and performance: Memory Saver and motion.

use rune::tok::type_;
use rune::{Drag, El, Phase, col, el, row, text};
use rune_kit::SegmentedControl;

use crate::app::AppState;
use crate::ui::components::widgets::{setting, setting_icon, switch_kit};
use crate::ui::icons;
use crate::ui::internal::PageContext;
use crate::ui::theme::{self, Theme};
use elaris_core::appearance::{AccentColor, Appearance, GlassStyle};
use elaris_core::settings::{NewTabPosition, PageStyle, SidebarSide, TabLayout};

use super::section;

const ACCENT_SWATCH_SIZE: f32 = 36.0;
const ACCENT_DOT_SIZE: f32 = 20.0;
const ACCENT_GAP: f32 = 5.0;
/// Each choice takes an equal share of the control column.
const PAGE_STYLE_WIDTH: f32 = (theme::PAGE_CONTROL_WIDTH - PAGE_STYLE_GAP) / 2.0;
const PAGE_STYLE_HEIGHT: f32 = 50.0;
const PAGE_STYLE_GAP: f32 = rune::tok::space::S3;
const PAGE_STYLE_SIDEBAR: f32 = 22.0;
const PAGE_STYLE_LINE: f32 = 3.0;
const PAGE_STYLE_INSET: f32 = 4.0;
/// The widths of the three text lines in the sidebar sketch, shortening downwards.
const PAGE_STYLE_LINE_WIDTHS: [f32; 3] = [14.0, 12.0, 10.0];
const SLIDER_HEIGHT: f32 = 22.0;
const SLIDER_TRACK: f32 = 4.0;
const SLIDER_KNOB: f32 = 16.0;
const SLIDER_TRAVEL: f32 = theme::PAGE_CONTROL_WIDTH - SLIDER_KNOB;

pub(super) fn sections(cx: &mut PageContext) -> Vec<El> {
    let mut sections = vec![appearance(cx)];
    // The glass material is macOS only, so Linux offers no glass settings.
    if !cfg!(elaris_ui = "linux") {
        sections.push(glass(cx));
    }
    sections.push(split_view(cx));
    sections.push(motion(cx));
    sections
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
    let force_dark = rune_kit::Switch::new(state.settings.force_dark)
        .standard()
        .label("Force dark mode")
        .hide_label()
        .on_toggle(|state: &mut AppState| state.set_force_dark(!state.settings.force_dark))
        .build(&switch_kit(&cx.kit));
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
    let rows = vec![
        setting(
            cx,
            "Theme",
            "Choose how Elaris and webpages look.",
            theme_control,
        ),
        setting(
            cx,
            "Force dark mode",
            "Darken web pages that don’t provide a dark theme of their own.",
            force_dark,
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
            "The page as a card",
            "A little room around the page, with rounded corners.",
            page_style_control(state.settings.page_style, cx.theme),
        ),
        setting(
            cx,
            "Accent colour",
            "Choose the colour used for interactive emphasis.",
            accent_color_control(state.settings.accent_color, cx.theme),
        ),
    ];
    section("Appearance", cx.theme, rows)
}

fn glass(cx: &mut PageContext) -> El {
    let state = cx.state;
    let glass = rune_kit::Switch::new(state.settings.glass_enabled)
        .standard()
        .label("Glass")
        .hide_label()
        .on_toggle_tx(|state: &mut AppState, tx| {
            state.set_glass_enabled(!state.settings.glass_enabled);
            tx.animated();
        })
        .build(&switch_kit(&cx.kit));
    let mut rows = vec![setting_icon(
        cx,
        &icons::GLASS,
        "Glass",
        "What is behind the window shows through the tabs and the frame, blurred",
        glass,
    )];
    // The other glass settings only show through glass, so their rows leave while glass is off.
    if state.settings.glass_enabled {
        let style = SegmentedControl::new(
            &GlassStyle::ALL.map(GlassStyle::label),
            GlassStyle::ALL
                .iter()
                .position(|style| *style == state.settings.glass_style),
        )
        .width(Some(theme::PAGE_CONTROL_WIDTH))
        .label("Glass style")
        .on_pick(|state: &mut AppState, index, _| state.set_glass_style(GlassStyle::ALL[index]))
        .build(&cx.kit);
        rows.push(setting_icon(
            cx,
            &icons::DROP,
            "Glass style",
            "Frosted, or the Mac's Liquid Glass",
            style,
        ));
        rows.push(setting_icon(
            cx,
            &icons::CONTRAST,
            "Transparency",
            "From the space's colour on the glass to clear glass",
            percent_slider(
                state,
                cx.theme,
                "transparency-slider",
                "Transparency",
                |state| 100u8.saturating_sub(state.settings.tint_opacity),
                |state, value| state.set_tint_opacity(100u8.saturating_sub(value)),
            ),
        ));
        rows.push(setting_icon(
            cx,
            &icons::GRAIN,
            "Grain",
            "A still noise over the glass",
            percent_slider(
                state,
                cx.theme,
                "grain-slider",
                "Grain",
                |state| state.settings.grain,
                |state, value| state.set_grain(value),
            ),
        ));
    }
    section("Glass", cx.theme, rows)
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
    let reduce_motion = setting(
        cx,
        "Reduce motion",
        "Minimise animations and transitions throughout Elaris.",
        reduce_motion,
    );
    let mut rows = super::memory::rows(cx);
    rows.push(reduce_motion);
    section("Performance", cx.theme, rows)
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

/// Two small window sketches, sidebar beside page, one per [`PageStyle`].
fn page_style_control(selected: PageStyle, theme: &Theme) -> El {
    let mut choices = row()
        .key("page-style-options")
        .role("radiogroup")
        .label("Page style")
        .w(theme::PAGE_CONTROL_WIDTH)
        .gap(PAGE_STYLE_GAP);
    for (index, style) in PageStyle::ALL.into_iter().enumerate() {
        let is_selected = selected == style;
        let card = style == PageStyle::Card;
        let mut lines = col()
            .key("sidebar")
            .w(PAGE_STYLE_SIDEBAR)
            .h_full()
            .padding(PAGE_STYLE_INSET + 2.0)
            .gap(PAGE_STYLE_LINE);
        for width in PAGE_STYLE_LINE_WIDTHS {
            lines = lines.child(
                el().w(width)
                    .h(PAGE_STYLE_LINE)
                    .radius(PAGE_STYLE_LINE / 2.0)
                    .fill(theme.text_muted),
            );
        }
        let mut page = el().key("page").grow(1.0).h_full().fill(theme.page);
        if card {
            // Concentric with the sketch's corners, one inset in.
            page = page.radius(rune::tok::radius::MD - PAGE_STYLE_INSET);
        }
        // Edge to edge, only a hairline parts the sidebar from the page.
        let divider = (!card).then(|| {
            el().key("divider")
                .w(1.0)
                .h_full()
                .fill(theme.control_border)
        });
        let sketch = row()
            .key("sketch")
            .size(PAGE_STYLE_WIDTH, PAGE_STYLE_HEIGHT)
            .padding(if card { PAGE_STYLE_INSET } else { 0.0 })
            .pl(0.0)
            .radius(rune::tok::radius::MD)
            .clip_rounded(rune::tok::radius::MD)
            .border(
                if is_selected { 2.0 } else { 1.0 },
                if is_selected {
                    theme.accent
                } else {
                    theme.control_border
                },
            )
            .a11y_hidden()
            .child(lines)
            .children(divider)
            .child(page);
        choices = choices.child(
            col()
                .key(format!("page-style-{}", style.config_key()))
                .w(PAGE_STYLE_WIDTH)
                .role("radio")
                .label(style.label())
                .checked(is_selected)
                .position_in_set(index + 1, PageStyle::ALL.len())
                .gap(rune::tok::space::S1)
                .items_center()
                .radius(rune::tok::radius::MD)
                .focus_ring(theme::FOCUS_WIDTH, 1.0, theme.focus)
                .on_press_tx(move |state: &mut AppState, tx| {
                    state.set_page_style(style);
                    tx.animated();
                })
                .child(sketch)
                .child(
                    text(style.label())
                        .font(type_::LABEL)
                        .color(if is_selected {
                            theme.text
                        } else {
                            theme.text_muted
                        })
                        .a11y_hidden(),
                ),
        );
    }
    choices
}

/// A 0 through 100 slider for the value `get` reads from the state and `set` writes back.
fn percent_slider(
    state: &AppState,
    theme: &Theme,
    key: &'static str,
    label: &'static str,
    get: fn(&AppState) -> u8,
    set: fn(&mut AppState, u8),
) -> El {
    let value = get(state).min(100);
    let position = SLIDER_KNOB / 2.0 + f32::from(value) / 100.0 * SLIDER_TRAVEL;
    el().key(key)
        .role("slider")
        .label(label)
        .range_value(f64::from(value), 0.0, 100.0)
        .range_step(1.0)
        .a11y_value(format!("{value}%"))
        .size(theme::PAGE_CONTROL_WIDTH, SLIDER_HEIGHT)
        .on_step(move |state: &mut AppState, steps, _| {
            let next = (i16::from(get(state)) + steps as i16).clamp(0, 100) as u8;
            set(state, next);
        })
        .on_drag(
            move |state: &mut AppState, drag: &mut Drag, _| match drag.phase {
                Phase::Down => {
                    drag.data = f32::from(get(state));
                    set(state, slider_value_at(drag.x));
                }
                Phase::Move => set(state, slider_value_at(drag.x)),
                Phase::Up if drag.cancelled() => set(state, drag.data.clamp(0.0, 100.0) as u8),
                Phase::Up => {}
            },
        )
        .child(
            el().key("track")
                .absolute(0.0, (SLIDER_HEIGHT - SLIDER_TRACK) / 2.0)
                .size(theme::PAGE_CONTROL_WIDTH, SLIDER_TRACK)
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
                .absolute(
                    position - SLIDER_KNOB / 2.0,
                    (SLIDER_HEIGHT - SLIDER_KNOB) / 2.0,
                )
                .size(SLIDER_KNOB, SLIDER_KNOB)
                .radius(SLIDER_KNOB / 2.0)
                .fill(theme.icon),
        )
}

/// The 0 through 100 value under a pointer `x` points from the slider's left edge.
fn slider_value_at(x: f32) -> u8 {
    (((x - SLIDER_KNOB / 2.0) / SLIDER_TRAVEL).clamp(0.0, 1.0) * 100.0).round() as u8
}
