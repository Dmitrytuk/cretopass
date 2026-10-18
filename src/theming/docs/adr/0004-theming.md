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
| `j`/`k` | Navigate rows (description + extra fields) |
| `Enter` | Copy selected row to clipboard → "Copied!" alert |
| `o` | Edit this node |
| `Esc` | Close overlay |

### Add/Edit form

| Keys | Action |
|------|--------|
| `Tab` / `Up` / `Down` | Switch field |
| `Enter` | Save |
| `Esc` | Cancel (confirmation if data changed) |
| `Left` / `Right` / `Home` / `End` | Cursor movement |
| `Backspace` / `Delete` | Delete character |
| `Delete` (on extra field row) | Confirm-delete popup |

### Home screen

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate menu |
| `Enter` | Select |
| `q` / `Esc` | Quit |
| `e` | Export active file |
| `c` / `Ctrl+C` | Change master password |
| `Ctrl+E` | Export active file (alternative) |
| `Ctrl+P` | Change password (alternative) |

## Dependencies

- `ratatui` 0.28 — styles
- `crossterm` 0.28 — key codes