# 0021 — Home modals report intents; the orchestrator performs them

The Home screen is eleven modals (the File Browser, three master-password
prompts, the Configuration and Export dialogs, a Confirmation, an Alert, the
About card) plus a menu. Every one of them used to take the terminal, read the
keyboard and then do its own work inline: decrypt the file, create it, re-encrypt
it, export it, parse an import, write the configuration, launch the Tree. One
file grew to 2490 lines, and not a single handler had a behaviour test, because
testing one meant a terminal, a real `age` file and a real click-shaped key
event walking through all of it.

**Decision:** a modal is pure input. `Modal::handle_key` reads a key and returns
a `HomeAction` — "unlock the file I was opened for", "export as XML", "drop this
path from the recent list". It never touches the filesystem, never draws, and
never builds another modal. Exactly one function, `orchestrator::execute`,
performs what the action asks: disk I/O, the blocking spinner, configuration
writes, screen transitions and launching the Tree. Reaching the same seam from
the other side, `file_browser::handle_key` already returned a
`FileBrowserAction` that the Home screen applied; the vault now follows the idiom
its own dependency set.

**Rejected: keep `terminal` and the I/O in each handler.** It is the shape the
code had, and it is why the flows were untestable. Passing a terminal around also
means the same three lines of validation, cursor bookkeeping and "re-show the
dialog with the error" appear in every prompt.

**Rejected: modals construct the next modal.** A confirmation that opens the
change-password dialog, an import that opens the file browser — the transitions
would form an eleven-by-eleven dependency graph among the modal modules, and the
flows would be spread across them. Transitions are knowledge of the flow, so the
orchestrator owns them: intents are named after what the user asked for
(`BeginChangePassword`, `ReopenCreateSecret`), not after the modal to install.

**Consequences.** `HomeAction` is the same vocabulary for the menu, which passes
`None` where a modal hands back the modal it came from — that `Option` is the
whole difference between "the screen itself" and "a window over it". Modals that
validate without leaving the prompt keep doing so themselves
(`CreateSecretPrompt` on a mismatched pair); the one check that needs the vault's
secret — the current master password — stays in the orchestrator, which is the
only thing that holds it. Because a modal now carries the file it was opened for
and the typed password, the actions carry no payload: `HomeAction::Unlock` needs
nothing but the modal it came with.