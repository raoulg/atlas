// Two levels of a signal, shown side by side: `pair` makes the list that holds them by its own code.

/** A level of a signal. */
export interface Level {
  readonly db: number;
}

function show(levels: Level[]): void {
  console.log(levels);
}

function pair(first: Level, second: Level): Level[] {
  const both: Level[] = [first, second];
  return both;
}

/** The two levels, shown side by side. */
export function paired(first: Level, second: Level): void {
  show(pair(first, second));
}
