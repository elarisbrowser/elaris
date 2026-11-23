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
