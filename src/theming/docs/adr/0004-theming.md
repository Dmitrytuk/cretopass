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
| `background.rs` | The app's Base background renderer: `paint_background(frame, area)` paints `ElementStyle::AppBackground` across a whole screen before anything else, so no cell falls through to the terminal's own background ([0018](../../../../docs/adr/0018-app-owned-background-fill.md)). |
| `icon.rs` | `Icon` enum (7 variants: selection arrow, expand/collapse, checkboxes, password marker). Renders to `&'static str`. |
| `key.rs` | `Action` enum with per-mode dispatch methods (`from_normal`, `from_search`, `from_pending_cut`, `from_pending_paste`, `from_pending_add`, etc.). Converts `KeyEvent` → semantic `Action`. |
| `palette_default.rs` | The named **Default** colour scheme: one TrueColor RGB constant per colour role, the cool-toned source of every colour ([0015](../../../../docs/adr/0015-default-color-scheme.md)). |
| `palette_cyber_orchid.rs` | An alternative **Cyber Orchid** warm scheme: the same roles re-tuned to the Cyber Orchid palette ([0016](../../../../docs/adr/0016-cyber-orchid-colour-scheme.md)). |
| `palette_astro_dark.rs` | The alternative **Astro Dark** cold scheme, derived from the VS Code Astro Dark theme ([0017](../../../../docs/adr/0017-astro-dark-colour-scheme.md)). Which scheme is active is chosen by the `pub use … as palette` line in `theming.rs`; nothing else changes. |
| `style.rs` | `ElementStyle` enum (67 variants) organized by UI area (Tree, Mode, FileBrowser, Home, Cursor, etc.). Each resolves to a concrete `ratatui::style::Style` built from `palette`. |
| `status_bar.rs` | The single shared status-bar renderer: `render_status_bar(frame, area, mode_str, mode_style, hint)` draws `[MODE] hints` for every screen (see [0014](../../../docs/adr/0014-status-bar-unification.md)). |
| `texts.rs` | All UI string constants: labels, hints, mode names, messages, menu items, keyboard help. ~178 lines. |

## Keybindings

### Tree — Normal mode

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate up/down |
| `h` | Collapse if expanded parent, else jump to parent |
| `l` | Expand (leaf: opens detail) |
| `Enter` | Toggle expand (parent) or detail (leaf) |
| `Ctrl+Enter` / `i` / `Tab` | Detail overlay for any node |
| `d d` | Cut selected node |
| `p` then `p`/`P`/`l` | Paste as sibling after / before / child (mode `PASTE`) |
| `a` then `a`/`A`/`l` | Add sibling after / before / child (mode `ADD`) |
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
| `c` / `y` | Copy selected row to clipboard → "Copied!" alert |
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