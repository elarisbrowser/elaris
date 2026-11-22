# Native UI design and verification

`src/ui/theme.rs` owns chrome surfaces, foregrounds and interaction colours. Window, page and
floating surfaces provide stable backgrounds for text. Hover and press use restrained alpha
overlays. Dragged tabs and keyboard selection use dedicated fills. Keep focus, selection and
control boundaries distinct from decorative dividers and shadows.

Build Rune Kit palettes through `Theme::kit`, using the Settings group or floating panel as the
seed surface. Rune Kit derives elevations and control geometry. Elaris supplies foregrounds for
menu labels, shortcuts and icons, plus the focus and interaction colours.

Use the shared native page title for full pages and Rune's body and label styles for common copy.
Reuse Rune spacing and radius tokens and Elaris's existing field, row and icon button dimensions.
Keep pointer-tracked geometry direct, spring surrounding layout, and respect system reduced
motion when revealing overlays or toolbars. The two theme tests use the [WCAG 2.2 contrast
criteria](https://www.w3.org/TR/WCAG22/#contrast-minimum) as a baseline. They model linear
source-over on stable backgrounds and include the surfaces Rune Kit derives.

## Tab layout

Settings → Appearance → Tab Layout switches the chrome for all windows. The preference is stored
as `[appearance].tab_layout` in `config.toml` and defaults to `"vertical"`. Both layouts use the
same open tabs and Split View state. The horizontal strip scrolls sideways with a trackpad or
Shift-wheel and reveals the newly selected tab when switching with the keyboard.

In the vertical sidebar, Settings → Appearance → New Tab button places New Tab above the tabs (the
default) or below them. The preference is stored as `[appearance].new_tab_position` in `config.toml`
as `"top"` or `"bottom"`. It sits at the start or end of the scrollable tab list, below any
favourites.

## Alignment lines

Gridline review, 21 November 2026:

| Area | Text line | Other lines |
|---|---:|---|
| Vertical sidebar | The shared `SIDEBAR_PADDING` inset | Search and tab icons share a centred icon column; section labels follow the icon column plus one gap. |
| Native page groups | The shared `PAGE_GROUP_PADDING` inset inside the group and section heading | Group controls end at the matching trailing inset. |
| Downloads popover | 12 pt inside the popover frame | The body content receives a 4 pt inset inside the popover's 8 pt content padding to match its title. |
| Window toolbar | Centred on the native titlebar controls | First and last controls use the frame's leading and trailing insets. |

The Downloads empty state was 4 pt left of the popover title. Its content container now supplies the
missing inset; active rows use the same container inset. The live macOS review covered the default
sidebar, Settings, and the empty Downloads popover. The Rune UI is native, so the web-only gridline
probe cannot measure it; narrow-window layouts, populated downloads, history, menus, and other
overlays were not reached in this pass.

## Split view grid

Split View opens with two panes. Settings → Split View sets the maximum between two and four;
`[split_view].max_panes` in `config.toml` accepts values through 64. With three or more panes,
panes share the width of their row. Drag another tab into the area where the next tile will appear, or click Add Pane to open a tab
picker. Drag the vertical divider to adjust the width of the first column.

## Repeatable graphical check

Set `ELARIS_DATA_DIR` to a new absolute temporary directory before launching a bridge-enabled
build. Put the download destination inside it and keep any explicit Ladybird profile override
inside it as well. Use the local `bridge/tests/smoke.html` fixture for Find and favicon checks.
Open Settings and an unknown `elaris://` page. Add two Favourites, then reopen
the browser to verify their order. Capture each surface in both forced appearances.

| Surface | What to inspect | November 2026 review |
|---|---|---|
| Sidebar, tabs and Favourites | Current and inactive labels, favicon, fallback icon, close, drag across sections, resize and collapse | Both themes on macOS Retina. Light cross-section drag and resize observed. |
| Omnibox | Empty and populated, text selection, current result, footer and focus return | Populated and selected text inspected in both themes. Creation and submission verified. |
| Find | Placeholder, disabled arrows, no matches, matches and selection | No-match and three-match light states, three-match dark state on the local page. |
| Quick Tab Switcher | Selected row, URL, wrapping, long list and modifier release | Both themes in Rune screenshots at 800×560. Native Control+Tab commit observed. |
| Settings and menus | Descriptions, selection, picker, keyboard focus and disabled rows | Both themes, including tab context menus. Light picker and narrower Settings layout inspected. Linux ARM64 Settings also reviewed in offscreen Mesa renders in both themes. |
| Downloads | Empty, active, completed and failed rows | Empty popover in both themes. macOS C++ download smoke covers transfer behaviour. |
| Native pages | Settings and unknown route, expanded and collapsed sidebar | Both themes, with collapsed layout inspected in dark mode. |

For the switcher's fleeting preview, use a restored session and Rune's CLI renderer. Set
`[appearance].theme` to `"system"` in the isolated config. Replay the following script with
`--script FILE --at 0.6 --size 800x560 --scale 1 --theme light --snap light.png`. Repeat with
`--theme dark`. Use `--tree` when checking that selected rows remain visible. Compare the
screenshots visually because text rasterization varies by display and platform.

```json
[
  {"t": 0.1, "hold": "ctrl"},
  {"t": 0.15, "type": "key", "name": "Tab", "ctrl": true},
  {"t": 0.75, "hold": ""}
]
```

Still to verify visually are live OS appearance changes, system reduced motion, screen-reader
navigation, tooltips and loading indicators across both themes, active download rows, and Linux
native windows. Headless tests cover reduced-motion omnibox and switcher behaviour. History
queries and search selection are covered by tests. The final close/quit check left a valid
session with the closed test page absent and no temporary session files. Rune reported a 12 ms
main-thread wait during that quit. The wait is limited to close and quit hooks.

Linux ARM64 passed formatting, locked metadata, check, Clippy and the default tests in Docker.
Bridge-enabled check and Clippy also passed. The cached Linux bridge predates the current download
ABI, so the feature-enabled tests need a fresh bridge build to link and run.

## Performance observations

The macOS debug review used `ELARIS_PERF=1` on a restored 24-tab fixture. Recorded frame-interval
medians were about 17 ms across most interaction windows. Some windows included idle gaps and
occasional long intervals, so the log cannot establish continuous 60 Hz motion or a performance
improvement. One idle process observation showed 0.0% CPU after 27 seconds. The Rune switcher
replay reported p95 CPU frame times of 6.1 ms in light and 4.9 ms in dark over 37 frames each.
Those are offscreen observations without a before/after comparison.

## Tab hover preview verification

Headless Rune tests cover the hover and dismissal delays, switching between tab sections,
native fallback, closing and dragging tabs, Split View focus, sidebar and window resizing,
and reduced motion. The headless Ladybird runtime smoke test checks that both visible views
supply independent thumbnails, a hidden view supplies none, and navigation produces a fresh
sample. With the bridge environment described in [the bridge README](../bridge/README.md), run
`cargo test --locked --features ladybird-bridge visible_views_sample_their_own_ladybird_frames -- --ignored`.

The locked Rust checks and headless Ladybird smoke tests ran on macOS. Linux x86-64 passed a
cross-target check. Linux runtime behaviour, live responsiveness, and appearance and motion in
native windows remain unverified.

## Internal pages

Each page has one route and one renderer:

1. Add the slug and tab title to `internal_pages!` in `src/internal.rs`.
2. Add a module under `src/ui/internal/` with `fn view(cx: &mut PageContext) -> PageLayout`.
3. Register it in `page` in `src/ui/internal/mod.rs`. The match is exhaustive, so a route
   without a renderer does not compile. List any `PageAction`s the page offers there too.

Right-clicking an internal page opens a native context menu (`src/ui/internal/menu.rs`): Back and
Forward when available, the page's actions, Copy Page Address, and links to the other internal
pages. It shares the web page menu's key, hover highlight and dismissal.

`PageContext` carries the app state, theme, a Rune Kit palette for the grouped surface, the
overlay list for dialogs and pickers, reduced motion and the compact flag. `PageLayout::new`
takes a stable scroll key, an icon and the content; `.toolbar(..)` pins controls under the title
and `.nav(..)` adds category navigation. Navigation sits in a sticky column beside the content
when the pane is at least `PAGE_NAV_BREAKPOINT` wide and folds into a scrolling row under the
title otherwise. The selected category is `AppState::page_category`; the page shows one category
at a time, like System Settings.
The frame in `layout.rs` handles the title, column width, alignment and scrolling.

Build content from `widgets.rs` rather than styling new surfaces:

| Widget | Use |
|---|---|
| `NavItem`, `nav`, `category` | Category navigation in a container whose selection and hover highlights glide between items, like the omnibox and tab switcher; and the selected category's heading and sections |
| `section` | A muted heading above one grouped surface |
| `rows` | A grouped surface of padded rows divided by inset hairlines |
| `setting`, `setting_with` | A title, description and trailing control; the control drops under the copy in compact panes |
| `actions` | Several buttons as one control |
| `message` | Centred empty, unavailable and unknown states; append a button to offer a way forward |
| `group`, `separator` | Custom rows, such as History entries |
| `switch_kit` | Accent-coloured switches |

All page measurements and type styles are the `PAGE_*` tokens in `src/ui/theme.rs`. Settings has
one module per category under `src/ui/internal/settings/`; add a category by listing its
`NavItem` and `sections` function in `CATEGORIES`.
