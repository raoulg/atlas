#!/bin/sh
# After a commit that raises the version in Cargo.toml, remove Atlas's own builds of every earlier
# version, in the dev and the release profile. Cargo names what it builds by the package's version,
# so each raise leaves a full set of test binaries behind that nothing else removes; dependencies
# stay, so the next build stays quick. It runs in the background, so a commit never waits on a
# build that holds cargo's lock; what it removed is in target/clean-old-builds.log.
git diff HEAD~1 HEAD -- Cargo.toml 2>/dev/null | grep -q '^+version = ' || exit 0
[ -d target ] || exit 0
nohup sh -c 'cargo clean -p atlas -p atlas-engine && cargo clean --release -p atlas -p atlas-engine' >target/clean-old-builds.log 2>&1 &
