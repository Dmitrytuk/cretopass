# 0014 — Unified status bar

Every screen drew its own bottom chrome: the Tree had a two-line status + help
bar with a per-mode badge and live context (`N items`, `/{query}`, node name),
Home had a static ` HOME ` label plus one help line, and the File Browser had a
two-line options bar (`Filter:`, search, `Hidden: on/off`, action hints). The
same mode could carry a different colour in each screen. This ADR replaces all
of them with one renderer and one colour vocabulary, so the status line looks
and behaves the same everywhere and is cheap to change.

**Decision:** a single `theming::render_status_bar(frame, area, mode_str,
mode_style, hint)` draws `[MODE] hints`. Each screen maps its own state to that
tuple; no screen draws status chrome itself. Layouts lose a row: content +
status, two rows.

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **One renderer in `theming/status_bar.rs`.** Screens call it with `(label, style, hint)`; they never build status spans themselves. | Same rationale as ADR 0004: presentation stays in `theming`, so a colour or spacing change happens once. |
| 2 | **Merge status and help into one row.** Layout becomes `[Min(1), Length(1)]`. | The help line duplicated what the status line could carry; two rows of chrome on every screen was pure cost. |
| 3 | **Badge colour = importance to the user, not the window.** Destructive red, Attention yellow, Active Input blue, Generator cyan, Add green, Passive dark gray. | One style means one thing in every screen; `CUT` is red wherever it appears. |
| 4 | **Reuse `ElementStyle::Mode*`.** Only `ModePendingPaste` (Cyan→Yellow), `ModeDetail` and `ModeHelp` (Magenta→DarkGray) change. | The existing palette already matched the categories except for three modes that were coloured by screen, not by meaning. |
| 5 | **Drop the Tree context statistics** (`N items`, `/{query}`, the node name). | Accepted loss: the line must be uniform across screens, and the count/query are not worth a dedicated row. |
| 6 | **File Browser collapses to the same line.** `Hidden: on/off` and `Filter:` are removed; the badge becomes `[PWD]` for the PwdKeeper filter and `[OPEN]` for All. | The filter is an active mode, so it belongs in the badge, not in a text indicator. |
| 7 | **`Alert` shows no badge.** `render_status_bar` omits the badge when `mode_str` is empty. | An alert is a transient dismissal prompt, not a mode; a `NORMAL` badge there was misleading. |
| 8 | **Hints are passive Gray.** All hint text uses `ElementStyle::StatusBarText` (Gray). | One colour for all hints; the badge carries the only accent. |
| 9 | **Hints live only in the status bar.** Every modal drops its inline hint line; a screen routes the hint for the active modal / overlay through the status bar instead. This includes the Tree save-error and discard-confirm overlays and the File Browser name/rename/delete/error popups, none of which are `InputMode`/`FileBrowserMode` variants. | The whole point of the unification: one place to read keys. The exception is the File Browser `Locations` popup, which can be tall enough to cover the status bar, so it keeps its own hint and the status bar shows only the badge. |

## Consequences

- Home's status line becomes dynamic: it reflects the open modal (`MENU`,
  `PASSPHRASE`, `CONFIG`, `DELETE`, `CHANGE PW`, `EXPORT`, …). While the File
  Browser is open it mirrors the browser's own line, using the same
  `file_browser::status_parts`, so the two can never disagree.
- `theming::texts::STATUS_HOME_LABEL` and the `ElementStyle` variants `HelpBar`,
  `HomeStatusBar`, `FbActiveSearch`, `HomeHelp`, `GrayHint` and `DismissHint` are
  removed; `MODE_MENU`, `MODE_OPEN`, `MODE_PWD`, `MODE_CREATE`, `MODE_IMPORT`,
  `MODE_PASSPHRASE`, `MODE_CONFIG`, `MODE_DELETE`, `MODE_CHANGE_PW`,
  `MODE_EXPORT` and the export hints are added. The `FB_FILTER_*`,
  `FB_HIDDEN_*`, `FB_HINT_LIST` and `FB_DELETE_CONFIRM_SUFFIX` constants are
  removed with their indicators.
- Modal and popup heights shrank by the removed hint row (one or two rows each).
- The File Browser list viewport grows by one row (three rows of chrome → two),
  which shifted its scroll-margin tests.
- There is now a single row for hints; on a narrow terminal a long hint (the
  File Browser's `[f] [/]search [D]disks [Ctrl+h]hidden …`) clips instead of
  wrapping to a second line.

## Rejected alternatives

- **A per-module renderer.** Reproduce the drift this ADR removes.
- **Keep a separate help bar.** Two rows of chrome per screen for no extra
  information.
- **Colour the badge by screen.** Makes `CUT` mean different things in different
  places and blocks a uniform theme.