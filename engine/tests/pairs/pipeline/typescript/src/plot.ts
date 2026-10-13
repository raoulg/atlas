// Figures of a sunspot series, as SVG documents: d3 computes the scales and the lines, and the
// document around them is text.

import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

import { scaleLinear, scaleUtc } from "d3-scale";
import { line } from "d3-shape";

import type { Month, Smoothed } from "./series.js";
import type { Settings } from "./settings.js";

const MARGIN = { top: 40, right: 20, bottom: 30, left: 50 } as const;
const MONTHLY = "#b3b3b3";
const SMOOTHED = "#1f77b4";

/** The monthly means as a thin grey line, with their smoothed mean over them. */
export function trend(series: readonly Month[], smoothed: readonly Smoothed[], settings: Settings): string {
  const first = series[0];
  const last = series.at(-1);
  if (first === undefined || last === undefined) throw new Error("trend: no months to draw");
  const { width, height } = settings;
  const bottom = height - MARGIN.bottom;
  const x = scaleUtc().domain([first.date, last.date]).range([MARGIN.left, width - MARGIN.right]);
  const y = scaleLinear()
    .domain([0, Math.max(...series.map((month) => month.mean))])
    .nice()
    .range([bottom, MARGIN.top]);
  const path = line<Smoothed>(
    (month) => x(month.date),
    (month) => y(month.mean),
  );
  const years = x.ticks(10).map((t) => `<text x="${x(t)}" y="${bottom + 18}" text-anchor="middle">${t.getUTCFullYear()}</text>`);
  const counts = y.ticks(5).map((n) => `<text x="${MARGIN.left - 6}" y="${y(n)}" text-anchor="end" dominant-baseline="middle">${n}</text>`);
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" font-family="sans-serif" font-size="12">`,
    `<rect width="${width}" height="${height}" fill="white"/>`,
    `<text x="${width / 2}" y="${MARGIN.top / 2}" text-anchor="middle" font-size="16">${escape(settings.title)}</text>`,
    `<line x1="${MARGIN.left}" x2="${width - MARGIN.right}" y1="${bottom}" y2="${bottom}" stroke="black"/>`,
    ...years,
    ...counts,
    `<text transform="translate(14,${(bottom + MARGIN.top) / 2}) rotate(-90)" text-anchor="middle">sunspot number</text>`,
    `<path d="${path(series) ?? ""}" fill="none" stroke="${MONTHLY}" stroke-width="0.6"/>`,
    `<path d="${path(smoothed) ?? ""}" fill="none" stroke="${SMOOTHED}" stroke-width="1.5"/>`,
    `<text x="${width - MARGIN.right}" y="${MARGIN.top}" text-anchor="end" fill="${MONTHLY}">monthly mean</text>`,
    `<text x="${width - MARGIN.right}" y="${MARGIN.top + 16}" text-anchor="end" fill="${SMOOTHED}">${settings.months}-month mean</text>`,
    `</svg>`,
    "",
  ].join("\n");
}

/** Writes `svg` to `to`, creating the directory it goes in; gives `to`. */
export async function saveSvg(svg: string, to: string): Promise<string> {
  await mkdir(dirname(to), { recursive: true });
  await writeFile(to, svg);
  return to;
}

function escape(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
