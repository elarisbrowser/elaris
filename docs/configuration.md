# Configuration

Most preferences are in Settings. A few advanced ones are set only in `config.toml`. The file's location is listed in
[the architecture notes](architecture.md#storage-and-privacy). Settings → Edit config.toml opens it, and
Reload config.toml applies changes without restarting. Invalid entries are reported on the Settings
page and skipped; the rest of the file still applies.

## Custom search engines

Add a table under `[search.engines]` for each provider, then select it in Settings, the omnibox's
search engine menu, or `[search].engine`:

```toml
[search]
engine = "kagi"

[search.engines.kagi]
name = "Kagi"
url = "https://kagi.com/search?q=%s"

[search.engines.docs]
url = "https://docs.rs/releases/search?query=%s"
```

| Key | Required | Meaning |
|---|---|---|
| table key (`kagi`) | Yes | What `[search].engine` refers to. Letters, digits, `-` and `_`. It cannot reuse a built-in key: `duckduckgo`, `google`, `bing` or `brave`. |
| `url` | Yes | An `http` or `https` URL containing `%s`. Each `%s` becomes the URL-encoded query. |
| `name` | No | The name shown in menus. Defaults to the table key. |

Custom engines follow the built-in ones in menus, in the order they appear in the file. They show a
generic search glyph instead of a brand mark.

## Keyboard shortcuts

`[keybindings]` replaces the keys of any action listed below. Actions not listed keep their
defaults.

```toml
[keybindings]
new_tab = "mod+k"                          # one chord
toggle_sidebar = ["mod+shift+s", "mod+\\"]  # several; menus show the first
copy_address = ""                          # no keys at all
```

A chord is `mod`, then optionally `shift` and `alt`, then one key, joined with `+`. `mod` is the
platform's shortcut modifier: Command on macOS and Control on Linux, so one file works on both.
`alt` (or `option`) is Option on macOS. Write the key unshifted, `[` rather than `{`, and the `+`
key as `plus`. Modifier names are case-insensitive. Chords that use `cmd` or `ctrl` are rejected,
so their meaning on another platform is never a surprise.

Overrides win: a chord you assign is removed from whichever action had it by default. These are
reported in Settings and skipped:

- an unknown action name;
- two actions claiming the same chord (the first in alphabetical order keeps it);
- a chord reserved for editing or the application: `mod` with `z`, `shift+z`, `x`, `c`, `v`, `a`
  or `q`, and on macOS `h` and `m`.

Escape, Control-Tab, and `mod+1` to `mod+8` (select a tab by position) are fixed. A chord you assign
to one of `mod+1` to `mod+8` takes precedence over tab selection.

| Action | Default |
|---|---|
| `new_window` | `mod+n` |
| `new_private_window` | `mod+shift+n` |
| `new_tab` | `mod+t` |
| `reopen_closed_tab` | `mod+shift+t` |
| `open_file` | `mod+o` |
| `close_tab` | `mod+w` |
| `focus_address_bar` | `mod+l` |
| `copy_address` | `mod+shift+c` |
| `find` | `mod+f` |
| `find_next` | `mod+g` |
| `find_previous` | `mod+shift+g` |
| `reload` | `mod+r` |
| `reload_bypassing_cache` | `mod+shift+r` |
| `back` | `mod+[` |
| `forward` | `mod+]` |
| `zoom_in` | `mod+=`, `mod+shift+=` |
| `zoom_out` | `mod+-` |
| `reset_zoom` | `mod+0` |
| `previous_tab` | `mod+shift+[` |
| `next_tab` | `mod+shift+]` |
| `select_last_tab` | `mod+9` |
| `toggle_sidebar` | `mod+s` |
| `open_history` | `mod+y` on macOS, `mod+h` on Linux |
| `open_settings` | `mod+,` |
| `bookmark_page` | `mod+d` |
| `bookmark_all_tabs` | `mod+shift+d` |
| `open_bookmarks` | `mod+alt+b` |
| `search_bookmarks` | `mod+shift+o` |
| `toggle_bookmarks_bar` | `mod+shift+b` |
| `open_task_manager` | none |

Menus and the shortcut hints in Elaris's own menus follow the keymap after Reload config.toml.
