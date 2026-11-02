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

function draw(gap: number): void {
  console.log(gap);
}

/** The gap between the two levels and the two levels side by side, each named before it is shown. */
export function named(first: Level, second: Level): void {
  const gap = first.db - second.db;
  draw(gap);
  const both: Level[] = [first, second];
  show(both);
}

/** The gap between the two levels and the two levels side by side, each written where it is shown. */
export function written(first: Level, second: Level): void {
  draw(first.db - second.db);
  show([first, second]);
}

/** The two levels, gathered in an array written out once and shown. */
export function gathered(first: Level, second: Level): void {
  const both: Level[] = [first, second];
  show(both);
}

/** The first level, and the second beside it where it is loud: the array the choice gives, written out in each arm. */
export function chosen(first: Level, second: Level, loud: boolean): void {
  const both: Level[] = loud ? [first, second] : [first];
  show(both);
}
