// The monthly mean sunspot number, a month at a time.
//
// SILSO's file has no header; each line holds seven numbers separated by semicolons and padded
// with spaces (year, month, decimal date, mean, standard deviation, observations, definitive),
// and a month without an observation has `-1` for its mean.

import { readFile } from "node:fs/promises";

const FIELDS = 7;
const MISSING = -1;

/** A month of SILSO's series that has a mean. */
export interface Month {
  /** The first day of the month, in UTC. */
  date: Date;
  mean: number;
  /** The standard deviation of the stations' daily numbers. */
  std: number;
  observations: number;
}

/** The mean sunspot number over the months centred on `date`. */
export interface Smoothed {
  date: Date;
  mean: number;
}

/** The months in SILSO's file at `path` that have a mean, in order. */
export async function load(path: string): Promise<Month[]> {
  return parse(await readFile(path, "utf8"));
}

/** The months in the text of SILSO's file that have a mean, in order. */
export function parse(text: string): Month[] {
  return text
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map(parseLine)
    .filter((month) => month.mean !== MISSING);
}

function parseLine(line: string): Month {
  const fields = line.split(";").map(Number);
  if (fields.length !== FIELDS || fields.some(Number.isNaN)) throw new Error(`SILSO: expected ${FIELDS} numbers in "${line}"`);
  const [year = 0, month = 0, , mean = 0, std = 0, observations = 0] = fields;
  return { date: new Date(Date.UTC(year, month - 1, 1)), mean, std, observations };
}

/** The mean over each run of `months` consecutive months, dated by the month at its centre. */
export function smooth(series: readonly Month[], months: number): Smoothed[] {
  const before = Math.floor(months / 2);
  const after = months - 1 - before;
  return series.slice(before, series.length - after).map((month, i) => ({ date: month.date, mean: average(series.slice(i, i + months)) }));
}

function average(series: readonly Month[]): number {
  return series.reduce((sum, month) => sum + month.mean, 0) / series.length;
}
