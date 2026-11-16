// The layout a page lays its pictures out with, on graphs the test hands it: `node layout.mjs PLAN_JS ELK_JS GRAPHS_JSON`
// reads a list of overviews, each `{root, columns}` (an ELK graph and the column of each box at its top), sets the
// options the page sets (`layOut` in `viewer/plan.js`, after the page's own constraints on the places of the world), lays
// each out with the ELK the page bundles, and prints for each either `{error}` or `{x}`: where each box at the top
// starts and ends.
import fs from "node:fs";
import {createRequire} from "node:module";

const [planJs, elkJs, graphsJson] = process.argv.slice(2);
const {layOut} = new Function("D", "window", fs.readFileSync(planJs, "utf8") + "\nreturn {layOut};")({nodes: [], edges: [], root: ""}, {});
const ELK = createRequire(import.meta.url)(elkJs);
const elk = new ELK();
// the page constrains a place it reads to the first layer and one it writes to the last (`worldNode`)
const PLACED = {in: "FIRST_SEPARATE", out: "LAST_SEPARATE"};
const out = [];
for (const {root, columns} of JSON.parse(fs.readFileSync(graphsJson, "utf8"))) {
  for (const c of root.children) if (PLACED[columns[c.id]]) c.layoutOptions = {"elk.layered.layering.layerConstraint": PLACED[columns[c.id]]};
  layOut(root, {loose: true, columnOf: c => columns[c.id]});
  try {
    const laid = await elk.layout(root);
    out.push({x: Object.fromEntries(laid.children.map(c => [c.id, [c.x, c.x + c.width]]))});
  } catch (e) {
    out.push({error: String(e).split("\n")[0]});
  }
}
console.log(JSON.stringify(out));
