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
| `icon.rs` | `Icon` enum (7 variants: selection arrow, expand/collapse, checkboxes, password marker). Renders to `&'static str`. |
| `key.rs` | `Action` enum with per-mode dispatch methods (`from_normal`, `from_search`, `from_pending_cut`, `from_pending_paste`, `from_pending_add`, etc.). Converts `KeyEvent` → semantic `Action`. |
| `style.rs` | `ElementStyle` enum (75 variants) organized by UI area (Tree, Mode, FileBrowser, Home, Cursor, etc.). Each resolves to a concrete `ratatui::style::Style`. |
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