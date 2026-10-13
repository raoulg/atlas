#!/usr/bin/env -S npx tsx
// The command line: a run names the source and the files, and hands them down.

import { parseArgs } from "node:util";

import { download } from "./download.js";
import { saveSvg, trend } from "./plot.js";
import { load, smooth } from "./series.js";
import { DEFAULT_SETTINGS, type Settings } from "./settings.js";

const SILSO = "https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv";
const RAW = "data/raw/sunspots.csv";
const FIGURE = "figures/sunspots.svg";

/** Downloads SILSO's monthly sunspot numbers, smooths them and saves their trend. */
async function main(argv: string[]): Promise<void> {
  const { values } = parseArgs({
    args: argv,
    options: {
      raw: { type: "string" },
      figure: { type: "string" },
      months: { type: "string" },
    },
  });
  const settings: Settings = { ...DEFAULT_SETTINGS, months: monthsOf(values.months) };
  const series = await load(await download(SILSO, values.raw ?? RAW));
  const smoothed = smooth(series, settings.months);
  await saveSvg(trend(series, smoothed, settings), values.figure ?? FIGURE);
}

/** The number of months `--months` asks to smooth over. */
function monthsOf(option: string | undefined): number {
  if (option === undefined) return DEFAULT_SETTINGS.months;
  const months = Number(option);
  if (!Number.isInteger(months) || months < 1) throw new Error(`--months: expected a whole number of months, got ${option}`);
  return months;
}

await main(process.argv.slice(2));
