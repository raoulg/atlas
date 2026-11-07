import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

import { RAW } from "./series.js";

/** Fetches `url` and writes its body to `to` as it came, where `load` reads by default, creating the directory it goes in; gives `to`. */
export async function download(url: string, to: string = RAW): Promise<string> {
  const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  await mkdir(dirname(to), { recursive: true });
  await writeFile(to, new Uint8Array(await response.arrayBuffer()));
  return to;
}
