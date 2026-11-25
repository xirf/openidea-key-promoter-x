# Key Promoter X for OpenIDEA

An OpenIDEA WebAssembly plugin port of **[IntelliJ Key Promoter X](https://github.com/halirutan/IntelliJ-Key-Promoter-X)** — learn keyboard shortcuts naturally whenever you use the mouse.

## Features

- **Non-intrusive shortcut notifications** when you trigger an IDE action via menu, toolbar, context menu, Search Everywhere, or other UI controls.
- **Interactive notification buttons**:
  - **Suppress Alert** — stop showing tips for that action.
  - **Show Hitlist** — jump straight to the Key Promoter X tool window.
  - **Snooze** — temporarily pause notifications.
- **Key Promoter X Tool Window** (right dock, two tabs):
  - **Key Promoter X (Missed Shortcuts)**: sortable table tracking `Action`, `Shortcut`, `Missed`, and `Hits` (keyboard uses). Double-click a row (or use the context menu) to suppress it.
  - **Suppressed**: list of silenced actions. Double-click an item or click **Restore All** to reactivate tips while keeping accumulated counts.
- **Missing shortcut suggestions**: when you click an action without an assigned shortcut multiple times, Key Promoter X prompts you to assign one in **Settings › Keymap**.
- **Hard Mode**: optional blocking confirmation dialog instead of a balloon notification when a shortcut is missed.
- **Status bar indicator & quick menu (`KPX`)**: shows total missed count or `KPX: Snoozed`, with one-click access to the hitlist, suppressed list, snooze toggle, suppress last action, reset, and credits.
- **Configurable settings** under **Settings › Plugins › Key Promoter X**.

## Capabilities

This plugin requests **no high-risk capabilities**:

| Capability | Risk | Why it is needed |
| --- | --- | --- |
| `events.input` | Medium | Subscribes to `action-performed` events so the plugin knows which action ran, its source (`menu`, `toolbar`, `context-menu`, `search`, `keyboard`, `other`), and its current keyboard shortcut. |
| `storage` | Low | Persists missed/hit counters, suppressed actions, unmapped shortcut counts, and snooze state across restarts and idle instance reclaim. |

## Settings

| Setting | Default | Description |
| --- | --- | --- |
| `showTipsClickCount` | `1` | Show a notification every Nth mouse click on an action with a shortcut. |
| `proposeToCreateShortcutCount` | `3` | Suggest creating a shortcut after clicking an unmapped action N times (`0` disables). |
| `hardMode` | `false` | Show a blocking confirmation dialog instead of a notification balloon. |
| `menusEnabled` | `true` | Show tips for main menu actions. |
| `toolbarButtonsEnabled` | `true` | Show tips for toolbar buttons. |
| `contextMenuEnabled` | `true` | Show tips for right-click context menu items. |
| `searchEnabled` | `true` | Show tips for Search Everywhere / Find Action invocations. |
| `otherButtonsEnabled` | `true` | Show tips for tool window and other UI buttons. |

## Differences from IntelliJ Key Promoter X

Because OpenIDEA runs plugins inside a sandboxed WebAssembly component API (`openidea:plugin@1.14.0`):
- **Edit Keybinding link**: OpenIDEA notifications cannot open **Settings › Keymap** to a specific action without the high-risk `ide.commands` capability, so this plugin intentionally omits `ide.commands` and directs users to **Settings › Keymap**.
- **Presentation / Distraction-Free Mode detection**: Not yet exposed in OpenIDEA's plugin API; use **Snooze Notifications** from the tool window, notification button, or status bar menu instead.

## Building & Local Development

Requires Rust with the `wasm32-wasip2` target and [`openidea-plugin`](https://github.com/l0ng-ai/openidea):

```sh
# Run unit tests
cargo test

# Build dist/plugin.wasm and validate the manifest
openidea-plugin build
openidea-plugin check

# Link into your local OpenIDEA installation for testing
openidea-plugin link .

# Package a signed .oiplugin archive
openidea-plugin pack . --key <your.key> --publisher xirf
```

## Credits & License

This plugin is a port of **[IntelliJ Key Promoter X](https://github.com/halirutan/IntelliJ-Key-Promoter-X)** and carries forward its **BSD 3-Clause License** (see [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md)):

- **Patrick Scheibe (`halirutan`)** — Author and maintainer of [IntelliJ Key Promoter X](https://github.com/halirutan/IntelliJ-Key-Promoter-X)
- **Dmitry Kashin** — Original author of [Key Promoter](https://code.google.com/p/key-promoter/)
- **Athiele (`athiele`)** — Maintainer of [key-promoter-fork](https://github.com/athiele/key-promoter-fork/commits/master)
