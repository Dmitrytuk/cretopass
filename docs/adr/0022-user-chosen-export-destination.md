# 0022 — User-chosen export destination

Export writes a Node (or the whole open tree) as plain text in JSON, XML, or
CSV. The file name and directory used to be derived from the open secret file:
the export landed beside the vault with the extension swapped for the format's
(see the glossary's `Export` term before this ADR). The user could not say where
the file went, and exporting twice silently overwrote the previous file. The
File Browser already existed for the mirror-image flows (Open, Create, Import),
so the export was the only flow whose destination was invisible.

**Decision:** the destination is named by the user in the File Browser, for both
the Home export (the whole vault tree) and the Tree export (one Node, with or
without its children). The flow becomes scope → format → destination → write,
and `FileBrowserMode::ExportFile { format }` is the mode that names the file.

Rules that follow from it:

- **One place knows the naming.** `file_handling::export` owns both rules:
  `suggest_export_name` (the secret file's sanitised stem, the Node's sanitised
  name when a single Node is exported, and the format's extension) and
  `output_path` (the directory and the **verbatim** stem of the path the user
  named, with the format's extension). The browser and the writer both call them.
  Only the *suggestion* sanitises, because it becomes a file name unasked: a
  source named `my vault.age` suggests `my_vault.json`, while a destination the
  user types keeps its typed stem verbatim (only the extension is replaced).
- **The format still owns the extension.** `output_path` replaces whatever
  extension was typed, so a name alone never changes what is written. The browser
  therefore resolves the typed name to the path it will actually write — its
  existence check, its confirmation, and its title (which names the format) all
  speak about that resolved path.
- **The open directory is the destination.** The browser opens on the file list,
  in the secret file's directory; navigating into a directory makes that
  directory the place the export lands. The file name is shown in the path bar,
  resolved to the format's extension, so the destination is legible without
  opening a dialog. `n` edits the name (Enter accepts the draft and returns to
  the list), and `e` writes to the directory now open. Enter on a file row does
  nothing: a file is not a target here.
- **Overwriting is asked about, not assumed.** An existing plain-text target
  raises a confirmation popup inside the browser (`y`/`Enter`), mirroring the
  browser's delete confirmation; the selection is reported only after the
  confirmation.
- **An export never overwrites a secret file.** A target that is a secret file
  (content-detected via `is_age_file`) is refused with an alert. The check lives
  in `export_tree`, so it covers every caller, and it protects any vault — not
  just the open one. This became reachable precisely because the destination is
  free now: a secret file may carry any name, including one an export produces.
- **The Tree hosts the File Browser.** Home already had it as a modal; the Tree
  gained `Modal::ExportDestination`, which carries the chosen scope and format
  across the browser step and reports them together with the path. The browser's
  session memory (last directory, hidden entries) travels into and out of
  `TreeState` the same way the generator settings do.
- **The Home wizard's confirmation step is gone.** "Are you sure you want to
  export as JSON?" added a prompt in front of the naming and overwrite
  confirmations that now carry the decision.

**Rejected alternatives:**

- *Directory-only pick, app-derived name*: needs a new "choose this directory"
  action in the browser, where the name input already returns a full path.
- *Keep deriving the path*: the status quo; nothing to review when the path is
  free, and no way to export somewhere other than beside the vault.
- *Silent overwrite*: one mistyped name silently destroys an unrelated file.
- *Never overwrite (auto-suffix)*: re-exporting the same scope litters the
  directory and cannot update a file in place.
- *Refuse in the browser instead of in the export layer*: earlier and friendlier,
  but the guard would not cover other callers of `export_tree`.

## Consequences

- Export may write anywhere the user can write. `MSG_NO_ACTIVE_FILE` disappears.
  The Tree needs an open vault only to seed the suggested name and starting
  directory; Home still refuses to open the browser without one, because there is
  nothing to export.
- The File Browser contract "Enter in the name input returns `Selected` only when
  no file with that name exists" (ADR 0007) gains an exception for the export
  mode — a re-export must be able to land on the file it produced last time — and
  its meaning narrows: in `ExportFile` mode Enter merely accepts the name, while
  `e` performs the write.
- `FileBrowserMode` carries the format for that one mode, so the modes are no
  longer a flat list of three verbs.
- Home's `Modal::FileBrowser` variant is boxed: the browser is the largest modal,
  and the added confirmation field pushed the enum over the size lint's threshold.
