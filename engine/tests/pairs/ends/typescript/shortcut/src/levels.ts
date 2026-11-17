// Levels measured in decibels, and the loudest of them, printed.

/** One level, as it was measured. */
export interface Level {
  readonly db: number;
}

/** The loudest of the levels, in decibels. */
export function loudest(levels: Level[]): void {
  console.log(Math.max(...levels.map((level) => level.db)));
}
