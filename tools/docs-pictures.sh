#!/bin/sh
# Before a commit that raises the version in Cargo.toml, draw the pictures the README, the Pages and
# the guide show with the build that commit makes (tools/docs_pictures.py), and stage them. When a
# picture draws otherwise than before, the commit stops and names it with the text that shows it:
# reread that text, change what no longer holds, and commit again.
git diff --cached -U0 -- Cargo.toml | grep -q '^+version = ' || exit 0
cargo build --release -q -p atlas || exit 1
uv run tools/docs_pictures.py target/release/atlas
status=$?
git add Cargo.lock docs/img docs/showcase docs/index.html
exit $status
