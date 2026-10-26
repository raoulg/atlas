# Changes, one file per issue

Every issue that lands on `next` leaves a note here, `<issue>.md`, so that the report of a stable release is put
together from what is already written rather than written afresh. A stable release's notes are
`docs/releases/<version>.md`: what the release is, and its main points, each gathering the issues it rests on and the
pictures of their notes and of what `tools/release_report.py` draws of the changes in what Atlas reads, draws and
checks. `tools/release_page.py` makes them a page of the documentation site, `releases/<version>/`, which the GitHub
release links to and Atlas Studio shows when it offers the update.

A note is short and written for someone who uses Atlas, not for its author:

```markdown
---
issue: 208
title: Studio: a workspaces page (⌘O) and Open Recent in the menu
area: studio            # engine, viewer, cli, mcp, studio, docs or release
commit: 3169677         # the commit that serves the issue; the merge that landed it beside it
pictures: [208-workspaces.jpg]
---
What a user can do now, or sees differently, in a sentence or three: before, and after.
```

The picture of a note is chosen by what changed. A change to what Atlas reads, draws or checks is drawn by
`uv run tools/release_report.py BEFORE AFTER <first>^ <last> <dir> --issues <issue>` (the coarsest view in which each
changed fact is a box or a line of its own; see the tool's own description); the note names the view it opens
(`view:`, Atlas's view format) and the release draws it again with the released builds. A change to Studio's or the
page's interface is a screenshot of the app, light theme, without personal data (no prompt that shows an address, no
private paths beyond the user's home). A change nothing shows (performance, an internal rule) says so and shows what
does: a timing, a command's output before and after.
