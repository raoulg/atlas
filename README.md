# Atlas's performance, measured every night on `next`

Written by `tools/perf.py` from `history.jsonl`, one line per run; how it measures and judges is in
`docs/reference/performance.md`. An amber dot is a run past its baseline (dashed), measured again on its
runner against the last commit within it; a red dot is one that measurement reproduced.

Last run: 2026-10-29T14:06Z, `1c02314`, INTEL(R) XEON(R) PLATINUM 8573C, 4 cores. Runs kept: 1.

## The release build

| measure | last | baseline | change | runs |
| --- | ---: | ---: | ---: | --- |
| `build/release/peak_rss` | 2546 MB | – | – | ![build/release/peak_rss](charts/build-release-peak_rss.svg) |
| `build/release/seconds` | 311 s | – | – | ![build/release/seconds](charts/build-release-seconds.svg) |

## Readings

| measure | last | baseline | change | runs |
| --- | ---: | ---: | ---: | --- |
| `read/atlas/cold/peak_rss` | 524 MB | – | – | ![read/atlas/cold/peak_rss](charts/read-atlas-cold-peak_rss.svg) |
| `read/atlas/cold/phase/check` | 0.04 s | – | – |  |
| `read/atlas/cold/phase/flow` | 6.30 s | – | – |  |
| `read/atlas/cold/phase/page` | 0.19 s | – | – |  |
| `read/atlas/cold/phase/read` | 52.62 s | – | – |  |
| `read/atlas/cold/seconds` | 59.29 s | – | – | ![read/atlas/cold/seconds](charts/read-atlas-cold-seconds.svg) |
| `read/atlas/warm/peak_rss` | 522 MB | – | – | ![read/atlas/warm/peak_rss](charts/read-atlas-warm-peak_rss.svg) |
| `read/atlas/warm/phase/check` | 0.04 s | – | – |  |
| `read/atlas/warm/phase/flow` | 6.17 s | – | – |  |
| `read/atlas/warm/phase/page` | 0.20 s | – | – |  |
| `read/atlas/warm/phase/read` | 0.18 s | – | – |  |
| `read/atlas/warm/seconds` | 6.62 s | – | – | ![read/atlas/warm/seconds](charts/read-atlas-warm-seconds.svg) |
| `read/goad_toolkit/cold/peak_rss` | 78 MB | – | – | ![read/goad_toolkit/cold/peak_rss](charts/read-goad_toolkit-cold-peak_rss.svg) |
| `read/goad_toolkit/cold/phase/check` | 0.00 s | – | – |  |
| `read/goad_toolkit/cold/phase/flow` | 0.02 s | – | – |  |
| `read/goad_toolkit/cold/phase/page` | 0.02 s | – | – |  |
| `read/goad_toolkit/cold/phase/read` | 1.00 s | – | – |  |
| `read/goad_toolkit/cold/seconds` | 1.05 s | – | – | ![read/goad_toolkit/cold/seconds](charts/read-goad_toolkit-cold-seconds.svg) |
| `read/goad_toolkit/warm/peak_rss` | 38 MB | – | – | ![read/goad_toolkit/warm/peak_rss](charts/read-goad_toolkit-warm-peak_rss.svg) |
| `read/goad_toolkit/warm/phase/check` | 0.00 s | – | – |  |
| `read/goad_toolkit/warm/phase/flow` | 0.02 s | – | – |  |
| `read/goad_toolkit/warm/phase/page` | 0.02 s | – | – |  |
| `read/goad_toolkit/warm/phase/read` | 0.01 s | – | – |  |
| `read/goad_toolkit/warm/seconds` | 0.05 s | – | – | ![read/goad_toolkit/warm/seconds](charts/read-goad_toolkit-warm-seconds.svg) |
| `read/ordash/cold/peak_rss` | 32 MB | – | – | ![read/ordash/cold/peak_rss](charts/read-ordash-cold-peak_rss.svg) |
| `read/ordash/cold/phase/check` | 0.00 s | – | – |  |
| `read/ordash/cold/phase/flow` | 0.00 s | – | – |  |
| `read/ordash/cold/phase/page` | 0.01 s | – | – |  |
| `read/ordash/cold/phase/read` | 0.33 s | – | – |  |
| `read/ordash/cold/seconds` | 0.35 s | – | – | ![read/ordash/cold/seconds](charts/read-ordash-cold-seconds.svg) |
| `read/ordash/warm/peak_rss` | 31 MB | – | – | ![read/ordash/warm/peak_rss](charts/read-ordash-warm-peak_rss.svg) |
| `read/ordash/warm/phase/check` | 0.00 s | – | – |  |
| `read/ordash/warm/phase/flow` | 0.00 s | – | – |  |
| `read/ordash/warm/phase/page` | 0.01 s | – | – |  |
| `read/ordash/warm/phase/read` | 0.00 s | – | – |  |
| `read/ordash/warm/seconds` | 0.02 s | – | – | ![read/ordash/warm/seconds](charts/read-ordash-warm-seconds.svg) |
| `read/pluim/cold/peak_rss` | 72 MB | – | – | ![read/pluim/cold/peak_rss](charts/read-pluim-cold-peak_rss.svg) |
| `read/pluim/cold/phase/check` | 0.00 s | – | – |  |
| `read/pluim/cold/phase/flow` | 0.00 s | – | – |  |
| `read/pluim/cold/phase/page` | 0.03 s | – | – |  |
| `read/pluim/cold/phase/read` | 1.70 s | – | – |  |
| `read/pluim/cold/seconds` | 1.74 s | – | – | ![read/pluim/cold/seconds](charts/read-pluim-cold-seconds.svg) |
| `read/pluim/warm/peak_rss` | 38 MB | – | – | ![read/pluim/warm/peak_rss](charts/read-pluim-warm-peak_rss.svg) |
| `read/pluim/warm/phase/check` | 0.00 s | – | – |  |
| `read/pluim/warm/phase/flow` | 0.00 s | – | – |  |
| `read/pluim/warm/phase/page` | 0.03 s | – | – |  |
| `read/pluim/warm/phase/read` | 0.01 s | – | – |  |
| `read/pluim/warm/seconds` | 0.05 s | – | – | ![read/pluim/warm/seconds](charts/read-pluim-warm-seconds.svg) |
| `read/vectormesh/cold/peak_rss` | 92 MB | – | – | ![read/vectormesh/cold/peak_rss](charts/read-vectormesh-cold-peak_rss.svg) |
| `read/vectormesh/cold/phase/check` | 0.00 s | – | – |  |
| `read/vectormesh/cold/phase/flow` | 0.02 s | – | – |  |
| `read/vectormesh/cold/phase/page` | 0.02 s | – | – |  |
| `read/vectormesh/cold/phase/read` | 1.50 s | – | – |  |
| `read/vectormesh/cold/seconds` | 1.55 s | – | – | ![read/vectormesh/cold/seconds](charts/read-vectormesh-cold-seconds.svg) |
| `read/vectormesh/warm/peak_rss` | 41 MB | – | – | ![read/vectormesh/warm/peak_rss](charts/read-vectormesh-warm-peak_rss.svg) |
| `read/vectormesh/warm/phase/check` | 0.00 s | – | – |  |
| `read/vectormesh/warm/phase/flow` | 0.02 s | – | – |  |
| `read/vectormesh/warm/phase/page` | 0.02 s | – | – |  |
| `read/vectormesh/warm/phase/read` | 0.01 s | – | – |  |
| `read/vectormesh/warm/seconds` | 0.06 s | – | – | ![read/vectormesh/warm/seconds](charts/read-vectormesh-warm-seconds.svg) |

## Tests

| measure | last | baseline | change | runs |
| --- | ---: | ---: | ---: | --- |
| `tests/atlas-code/atlas-code/seconds` | 0.88 s | – | – |  |
| `tests/atlas-code/exit/seconds` | 0.32 s | – | – |  |
| `tests/atlas-code/help/seconds` | 0.02 s | – | – |  |
| `tests/atlas-code/history/seconds` | 0.44 s | – | – |  |
| `tests/atlas-code/mcp/seconds` | 0.68 s | – | – |  |
| `tests/atlas-code/serve/seconds` | 12.53 s | – | – |  |
| `tests/atlas-code/view/seconds` | 0.23 s | – | – |  |
| `tests/atlas-engine/arrows/seconds` | 0.20 s | – | – |  |
| `tests/atlas-engine/aside/seconds` | 0.57 s | – | – |  |
| `tests/atlas-engine/cache/seconds` | 0.49 s | – | – |  |
| `tests/atlas-engine/changes/seconds` | 0.20 s | – | – |  |
| `tests/atlas-engine/composition/seconds` | 0.73 s | – | – |  |
| `tests/atlas-engine/diff/seconds` | 1.33 s | – | – |  |
| `tests/atlas-engine/environment/seconds` | 0.05 s | – | – |  |
| `tests/atlas-engine/laws/seconds` | 0.09 s | – | – |  |
| `tests/atlas-engine/lib/seconds` | 0.34 s | – | – |  |
| `tests/atlas-engine/libraries/seconds` | 0.52 s | – | – |  |
| `tests/atlas-engine/library_depth/seconds` | 0.56 s | – | – |  |
| `tests/atlas-engine/merge/seconds` | 0.06 s | – | – |  |
| `tests/atlas-engine/named/seconds` | 0.95 s | – | – |  |
| `tests/atlas-engine/pairs/seconds` | 0.66 s | – | – |  |
| `tests/atlas-engine/pairs_aliases/seconds` | 0.59 s | – | – |  |
| `tests/atlas-engine/pairs_body/seconds` | 0.81 s | – | – |  |
| `tests/atlas-engine/pairs_figures/seconds` | 10.75 s | – | – |  |
| `tests/atlas-engine/pairs_loops/seconds` | 0.36 s | – | – |  |
| `tests/atlas-engine/pairs_pipeline/seconds` | 0.43 s | – | – |  |
| `tests/atlas-engine/pairs_python/seconds` | 5.67 s | – | – |  |
| `tests/atlas-engine/pairs_readback/seconds` | 0.36 s | – | – |  |
| `tests/atlas-engine/pairs_rust/seconds` | 1.72 s | – | – |  |
| `tests/atlas-engine/pairs_storage/seconds` | 0.45 s | – | – |  |
| `tests/atlas-engine/pairs_typescript/seconds` | 1.80 s | – | – |  |
| `tests/atlas-engine/parse/seconds` | 68.07 s | – | – |  |
| `tests/atlas-engine/parts/seconds` | 1.30 s | – | – |  |
| `tests/atlas-engine/plans/seconds` | 114 s | – | – |  |
| `tests/atlas-engine/principles/seconds` | 0.91 s | – | – |  |
| `tests/atlas-engine/prophetize/seconds` | 0.01 s | – | – |  |
| `tests/atlas-engine/python_reader/seconds` | 0.20 s | – | – |  |
| `tests/atlas-engine/readers/seconds` | 0.17 s | – | – |  |
| `tests/atlas-engine/remote/seconds` | 0.17 s | – | – |  |
| `tests/atlas-engine/resolution/seconds` | 0.53 s | – | – |  |
| `tests/atlas-engine/review/seconds` | 55.30 s | – | – |  |
| `tests/atlas-engine/rust_facts/seconds` | 0.08 s | – | – |  |
| `tests/atlas-engine/rust_reader/seconds` | 0.02 s | – | – |  |
| `tests/atlas-engine/sample/seconds` | 0.03 s | – | – |  |
| `tests/atlas-engine/story/seconds` | 0.02 s | – | – |  |
| `tests/atlas-engine/typescript/seconds` | 0.26 s | – | – |  |
| `tests/atlas-engine/wording/seconds` | 0.93 s | – | – |  |
| `tests/atlas-engine/workspace_index/seconds` | 0.18 s | – | – |  |
| `tests/total/seconds` | 287 s | – | – | ![tests/total/seconds](charts/tests-total-seconds.svg) |
