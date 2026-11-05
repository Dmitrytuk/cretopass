# 0023 — Sharing a vault subtree

Export ([0022](0022-user-chosen-export-destination.md)) writes a Node tree as
**plain text** (JSON/XML/CSV): it is unusable as a working secret file, and it
exposes every password in the clear on disk. There was no way to hand part of a
vault to another person as something they can open in cretopass.

**Decision:** a new **Share** flow writes a self-contained *share bundle* — a
folder holding a new, still-encrypted secret file for what is shared plus a
plain-text instruction — for two entry points: the Tree (`Ctrl+E`, then `s`,
sharing the selected Node alone or with its children) and the Home menu
(«Share active file», sharing the whole open vault).

A bundle is:

```
<dest>/                       <- the user names the bundle folder itself
    <stem>.cretopass          <- a normal vault (magic-header detected)
    HOW-TO-OPEN.txt
```

`<stem>` is the source secret file's **sanitised** stem, plus the sanitised Node
name when a single branch is shared (`vault_Work.cretopass`) — the same rule as
`file_handling::suggest_export_name` (`file_handling::suggest_share_name`). The
stem is sanitised because it also names the generated instruction, so a source
named `my vault.age` suggests `my_vault.cretopass`.

Rules that follow from it:

- **The recipient gets their own password.** The bundle's file is re-encrypted
  under a password typed at share time; the source master password is never
  involved. An empty password is allowed, but only after an explicit
  confirmation.
- **The password never lands in the bundle.** `HOW-TO-OPEN.txt` names the app,
  its version and feedback link, the project and installer links, the bundle
  file, and the steps to open it; a protected bundle says the password travels
  over a separate channel, a passwordless one says no password is needed.
- **One place knows the naming and the write.** `file_handling::share` owns
  `suggest_share_name` (the folder name) and `share_tree` (the write). The bundle's
  file goes through the same atomic temp + fsync + rename path as a normal save
  (`save_tree`), so a crash mid-write never leaves a truncated bundle; the
  instruction is written after it, so a failure there still leaves an openable
  vault.
- **The password is shared UI, the chrome is not.** A UI-agnostic `share` module
  owns the masked password pair (`Passphrase`, over `cursor::SecretFieldGroup`);
  the Tree and Home screens each wrap it in their own modals, chrome and actions
  (`TreeAction` / `HomeAction`). The Tree's scope is the same node / node-with-
  children choice as export, carried as `include_children`.
- **The File Browser names the folder.** `FileBrowserMode::ShareDestination`
  names the bundle folder, mirroring the export mode: `n` edits the name and `e`
  on the list (the export's write key) or Enter in the input commits it. An
  existing folder that is not empty asks for confirmation first, so a re-share
  can update the same bundle; a path that is an existing file — in particular
  another secret file — is refused.
- **Unsaved edits are included.** Like an export, a share takes the tree from
  memory.
- **The extension is a label.** The bundle's file is written with `.cretopass`
  rather than `.age`; detection is content-based (the age magic header), so the
  extension only tells a human which app opens it. The create-new-file flow is
  unchanged. (The File Browser's row hint is name-only and `.age`-based
  ([0024](0024-browsing-never-reads-file-contents.md)), so a `.cretopass` bundle
  is not highlighted in the list; opening it is still content-detected.)

**Rejected alternatives:**

- *A zip archive instead of / as well as a folder*: `age` ciphertext is
  incompressible, so a zip adds no size benefit and would need a dependency for
  no gain. A folder is transparent and needs nothing.
- *Pick a parent directory and derive the sub-folder name*: the File Browser's
  name input already returns a full path; naming the bundle folder directly
  matches export ([0022](0022-user-chosen-export-destination.md)) and needs no
  new browser action.
- *Fixed inner names (`secrets.age` + `README.txt`)*: `<stem>` keeps context
  when several bundles sit in one place.
- *Forbid a passwordless share*: inconsistent with the existing passwordless
  vault support; a confirmation is enough.
- *Refuse an existing destination folder*: then a re-share could never update
  the bundle it produced; asking keeps the update path.

## Consequences

- `FileBrowserMode` gains a destination mode for folders, so its mode set is no
  longer a flat list of verbs (following
  [0022](0022-user-chosen-export-destination.md)).
- The Tree gains `InputMode::PendingExport`: `Ctrl+E` is now a one-key prefix
  (`e` export, `s` share) instead of opening the export wizard directly.
- The Home menu gains a «Share active file» row between Export and Import.
- Masked password fields move from the vault widget into the shared `cursor`
  module (`SecretField`/`SecretFieldGroup`), because the Tree cannot depend on
  `vault` (the dependency is one-way `vault → tree`).
