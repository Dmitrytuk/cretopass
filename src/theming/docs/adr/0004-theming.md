# 0004 — theming module

Centralized theming layer with zero dependencies on other project modules.
All UI strings, styles, icons, and key-to-action dispatchers live here.

**Decision:** keep presentation strictly separate from logic. Every piece of UI
text lives in `texts.rs`, every style in `style.rs`, every icon in `icon.rs`,
and every keybinding mapping in `key.rs`. UI modules never define their own
colors or keyboard shortcuts inline.

**Rejected alternative:** defining styles and help text in each UI module.
Leads to duplication, inconsistent colors, and makes theme-switching
impossible without touching every module.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `background.rs` | The app's Base background renderer: `paint_background(frame, area)` paints `ElementStyle::AppBackground` (`BASE_BG`) across a whole screen before anything else, so no cell falls through to the terminal's own background ([0018](../../../../docs/adr/0018-app-owned-background-fill.md)). |
| `icon.rs` | `Icon` enum (7 variants: selection arrow, expand/collapse, checkboxes, password marker). Renders to `&'static str`. |
| `key.rs` | `Action` enum with per-mode dispatch methods (`from_normal`, `from_search`, `from_pending_cut`, `from_pending_paste`, `from_pending_add`, etc.). Converts `KeyEvent` → semantic `Action`. Also hosts the cross-screen input-path helpers: `is_clipboard_paste`, `wheel_nav_key`, and the shared `KeyRoute`/`route_key` classifier that keeps `Ctrl+V` out of every `Char(c)` arm. |
| `palette_default_dark.rs` | The **Default Dark** colour scheme: a `Palette` value defining every colour role — including the `base_bg` canvas and the `surface` panels — the cold scheme derived from the VS Code Astro Dark theme ([0015](../../../../docs/adr/0015-default-dark-colour-scheme.md)). |
| `palette_default_light.rs` | The **Default Light** colour scheme: the warm, off-white counterpart of Default Dark, the same roles with light values ([0019](../../../../docs/adr/0019-runtime-colour-scheme-switching.md)). |
| `theming.rs` | Owns the `Palette` type, the scheme-name constants, and the active-scheme cell (`palette()` / `set_palette()`); a scheme swap is one write ([0019](../../../../docs/adr/0019-runtime-colour-scheme-switching.md)). |
| `style.rs` | `ElementStyle` enum (69 variants) organized by UI area (Tree, Mode, FileBrowser, Home, Cursor, etc.). Each resolves to a concrete `ratatui::style::Style` built from the active colour scheme. |
| `status_bar.rs` | The single shared status-bar renderer: `render_status_bar(frame, area, mode_str, mode_style, hint)` draws `[MODE] hints` for every screen (see [0014](../../../../docs/adr/0014-status-bar-unification.md)). |
| `alert.rs` | The single shared alert card: `AlertKind` (`Success`/`Error`) and `render_alert(frame, area, message, kind)`. Both the Home and Tree alert modals draw through it, so a positive outcome is always green and a failure red. |
| `texts.rs` | All UI string constants: labels, hints, mode names, messages, menu items, keyboard help — plus `share_instruction()`, which assembles the share bundle's `HOW-TO-OPEN.txt` from those constants so the `file_handling` gateway owns no user-facing prose. |

## Icons

`icon.rs` is the one place a glyph is chosen. Icons must stay within **WGL4**
(or plain ASCII), because the console fonts Windows ships are WGL4-level: a
codepoint outside it has no glyph in the font, and Windows substitutes it from
Segoe UI Symbol/Emoji, drawing the boxed emoji form — usually double-width,
which also shifts the row. The expand/collapse pair is the WGL4-safe `►` / `▼`
(`U+25BA` / `U+25BC`); the boxed-emoji triangles `U+25B6` `▶` / `U+25C0` `◀`
are banned for exactly this reason.

## Alerts

`theming::render_alert` draws the one alert card both screens use: five rows
tall, three-fifths of the width, centred, titled `ALERT_TITLE`. The outcome is
carried by `AlertKind`, which maps to the block/text `ElementStyle`s:

| Kind | Border / title | Message | Reported for |
|------|----------------|---------|--------------|
| `Success` | `AlertSuccessBlock` (green) | `AlertSuccessText` (green, bold) | export, import, settings saved, copy |
| `Error` | `AlertErrorBlock` (red) | `AlertErrorText` (default) | refused actions, failed writes, a refused overwrite |

Home's `Modal::success` / `Modal::error` and the Tree's `Modal::success` /
`Modal::error` all build this card, so the same outcome never looks different
between the two screens. A copy result uses the same card: the orchestrator's
`report_copy` installs `Modal::success(COPY_CHECK)` or
`Modal::error(MSG_COPY_FAILED + error)`, replacing the old transient "Copied!"
popup.

## Keybindings

### Tree — Normal mode

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate up/down |
| `h` | Collapse if expanded parent, else jump to parent |
| `l` | Expand (leaf: opens detail) |
| `Enter` | Toggle expand (parent) or detail (leaf) |
| `Ctrl+Enter` / `i` / `Tab` | Detail overlay for any node |
| `d d` | Cut selected node (inert on the Root) |
| `p` then `p`/`P`/`l` | Paste as sibling after / before / child (mode `PASTE`; the Root accepts only `l`) |
| `a` then `a`/`A`/`l` | Add sibling after / before / child (mode `ADD`; the Root accepts only `l`) |
| `/` | Search |
| `Esc` | Clear search highlights |
| `q` / `Ctrl+C` | Quit |
| `H` (shift+h) | Return to home screen |
| `Ctrl+E` | Export node |

### Tree — Search mode

| Keys | Action |
|------|--------|
| `Enter` | Keep highlights, free navigation (back to Normal) |
| `Esc` | Clear search, return to Normal |
| Printable chars (incl. `j`/`k`) | Append to search query |
| `Backspace` | Remove last char from query |

While a search is active (after `Enter`, in Normal mode), `j`/`k` cycle
through matched rows only.

### Tree — `PendingCut` (after first `d`)

| Keys | Action |
|------|--------|
| `d` | Cut selected node |
| Any other key / `Esc` | Cancel, return to Normal |

### Detail overlay

| Keys | Action |
|------|--------|
| `j`/`k` / `↑` / `↓` | Navigate rows (a node's extra fields) |
| `Tab` / `Shift+Tab` | Navigate rows, wrapping (like the form's Nav level) |
| `c` / `y` | Copy selected row to the system clipboard → result modal ([0025](../../../../docs/adr/0025-clipboard-gateway.md)) |
| `i` / `Enter` | Edit this node |
| `Esc` | Close overlay |

### Add/Edit form

The form is a **live editor**: leaving the Input level commits the field's
text straight into the node. It has two levels: **Nav** (focus movement, form
actions) and **Input** (editing the focused field's text). It opens in Nav;
`i`/`Enter` enter Input, `Enter`/`Esc` commit and return to Nav.

| Level | Keys | Action |
|-------|------|--------|
| Nav | `j`/`k` / `Up` / `Down` / `Tab` / `Shift+Tab` | Switch field |
| Nav | `i` / `Enter` | Enter Input level (or open the field-type picker on "+ Add Field") |
| Nav | `u` / `r` | Undo / redo a committed change (Edit sessions only) |
| Nav | `Delete` (on extra field row) | Confirm-delete popup (deletion commits immediately) |
| Nav | `d` then `d` | Cut the focused extra field (mode `CUT`) |
| Nav | `p` then `p` / `P` | Paste the cut field after / before the focused field (mode `PASTE`) |
| Nav | `Esc` | Close (Edit → Detail; empty Add → Normal) |
| Input | Printable chars | Edit the focused field's text |
| Input | `Left` / `Right` / `Home` / `End` | Cursor movement |
| Input | `Backspace` / `Delete` | Delete character |
| Input | `Enter` / `Esc` | Commit the field and return to Nav |

### Home screen

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate menu |
| `Enter` / `l` / `→` | Select |
| `q` / `Esc` | Quit |
| `e` | Export active file |
| `c` / `Ctrl+C` | Change master password |
| `Ctrl+E` | Export active file (alternative) |
| `Ctrl+P` | Change password (alternative) |

## Dependencies

- `ratatui` 0.28 — styles
- `crossterm` 0.28 — key codes