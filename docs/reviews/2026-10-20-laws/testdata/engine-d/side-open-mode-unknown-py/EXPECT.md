Route: `modeunknown.run` (not the default `modeunknown.touch`)
- File(data/notes.txt): neither — the mode is a parameter fed from argv: "Where the code does not say which way a call goes, the place is in neither."
Observed (0.68.0): in (the touch is `disk_read`, verb "reads"). DEFECT (docs vs engine): effects.rs:313-318 reads every `open` without a literal mode (`None if file => DISK_READ`), and flow.rs:366 draws a disk touch with no direction as read.
