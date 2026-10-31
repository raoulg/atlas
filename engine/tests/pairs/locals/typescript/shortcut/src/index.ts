// A level of a signal, shown beside the quiet level: `pair` makes the list that holds them by its own code, from the level
// it is handed and the module's quiet level.

/** A level of a signal. */
export interface Level {
  readonly db: number;
}

const QUIET: Level = { db: 0 };

function show(levels: Level[]): void {
  console.log(levels);
}

function pair(first: Level): Level[] {
  const both: Level[] = [first, QUIET];
  return both;
}

/** The level, shown beside the quiet level. */
export function paired(first: Level): void {
  show(pair(first));
}

function draw(gap: number): void {
  console.log(gap);
}

/** The gap between the level and the quiet level and the two side by side, each named before it is shown. */
export function named(first: Level): void {
  const gap = first.db - QUIET.db;
  draw(gap);
  const both: Level[] = [first, QUIET];
  show(both);
}

/** The gap between the level and the quiet level and the two side by side, each written where it is shown. */
export function written(first: Level): void {
  draw(first.db - QUIET.db);
  show([first, QUIET]);
}

/** The two levels, gathered by pushing the second onto an array of the first, and shown. */
export function gathered(first: Level, second: Level): void {
  const both: Level[] = [first];
  both.push(second);
  show(both);
}

function add(levels: Level[], level: Level): void {
  levels.push(level);
}

/** The two levels, gathered by handing an array of the first to a function that adds the second, and shown. */
export function lent(first: Level, second: Level): void {
  const both: Level[] = [first];
  add(both, second);
  show(both);
}
