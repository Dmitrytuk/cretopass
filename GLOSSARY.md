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
rather than silently claimed. Distinct from the Tree's internal **Cut buffer**
and **field cut buffer**.
_Avoid_: Clipboard (ambiguous with the Tree buffers), paste buffer
