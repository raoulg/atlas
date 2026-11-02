// The command: load the shelf and show it.
import { render } from "./report/show.js";
import { load } from "./store/disk.js";

render(load("shelf.csv"));
