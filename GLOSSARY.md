# cretopass

Cross-context vocabulary for the cretopass application as a whole: its identity
and the ways users reach the project.

## Language

**Feedback channel**:
The single public place where users report bugs and request features — the
project's GitHub Issues. Security vulnerabilities do not go here; they are
reported privately through GitHub Private Vulnerability Reporting.
_Avoid_: Support, contact, bug tracker

**System clipboard**:
The operating system's shared copy buffer, reached only through the app-wide
`clipboard` gateway (`arboard` on every platform, with
`pbcopy`/`wl-copy`/`xclip`/`xsel` as a fallback). The Detail modal writes a
field's value to it on `c`/`y`, and a write that cannot happen is reported
rather than silently claimed. It is also read back into a field by a **Clipboard
paste**. Distinct from the Tree's internal **Cut buffer** and **field cut
buffer**.
_Avoid_: Clipboard (ambiguous with the Tree buffers), paste buffer

**Clipboard paste** / **Paste from the system clipboard**:
Reading the System clipboard's text into the focused field, triggered by `Ctrl+V`
or a terminal-native (bracketed) paste. It reaches only the app's text inputs —
the Tree form, the custom-field-name prompt, the generator's length row, tree
search, the five password prompts and the File Browser's name/rename/search —
and nowhere else. Multi-line text is joined into one line; the generator's length
row keeps digits only. Distinct from the Tree's **Paste** operations (`PasteStart`
/ `PasteSibling` / `PasteChild`, moving a Node from the Cut buffer) and from the
Form's field paste (moving an ExtraField from the field cut buffer): those move
data already inside the app, while a Clipboard paste brings outside text in.
_Avoid_: Paste (ambiguous with the Tree and Form Paste operations)
