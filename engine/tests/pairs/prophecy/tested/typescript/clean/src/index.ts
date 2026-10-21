/** How a report is kept: trimmed now. */
export function run(report: string): string {
  return report.trim();
}

/** @prophecy supersedes=run */
export declare function seal(report: string): string;
