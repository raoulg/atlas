#!/bin/sh
# Keeps target/ to what the current builds use. Cargo names what it builds by the package's version
# and by the hash of its dependencies and toolchain, so a raised version, an updated dependency or a
# new Rust each leave a full set of builds behind that nothing else removes.
# - After a commit that raises the version in Cargo.toml: Atlas's own builds of every earlier version
#   go, in the dev and the release profile; dependencies stay, so the next build stays quick.
# - After every commit and merge, with cargo-sweep installed: builds of a toolchain no longer installed
#   go, and the least recently used builds go until target/ is below 15 GB.
# It runs in the background, so a commit never waits on a build that holds cargo's lock; what it
# removed is in target/clean-old-builds.log.
[ -d target ] || exit 0
raised=no
[ "$1" = commit ] && git diff HEAD~1 HEAD -- Cargo.toml 2>/dev/null | grep -q '^+version = ' && raised=yes
nohup sh -c "
  if [ $raised = yes ]; then cargo clean -p atlas -p atlas-engine && cargo clean --release -p atlas -p atlas-engine; fi
  if command -v cargo-sweep >/dev/null; then cargo sweep --installed && cargo sweep --maxsize 15GB; fi
" >target/clean-old-builds.log 2>&1 &
