# Viewer scripts

The Atlas viewer includes these browser builds in its HTML so saved pages and Studio work without network access:

| File | Package | Version | Licence |
| --- | --- | --- | --- |
| `d3.min.js` | d3 | 7.9.0 | ISC (`LICENSE.d3`) |
| `highlight.min.js` | highlight.js | 11.9.0 | BSD-3-Clause (`LICENSE.highlight.js`) |
| `elk.bundled.js`, `elk-worker.min.js` | elkjs | 0.9.3 | EPL-2.0 (`LICENSE.elkjs`) |

The D3 and ELK files come from their npm distributions. The highlight.js browser build comes from the versioned cdnjs distribution. `cli/src/page.rs` embeds all four files in both saved and served pages.
