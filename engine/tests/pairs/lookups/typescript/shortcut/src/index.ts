/**
 * The levels of a signal, each looked up in a collection the module keeps: by a subscript, or by the method that looks
 * it up. `pushed` and `pushedLookedUp` look up in a list they change in place first, which starts at the module's quiet
 * level; `added` and `addedLookedUp` change the row they look up, and so the module's rows.
 */

/** A level of a signal. */
export interface Level {
  db: number;
}

const QUIET: Level = { db: 0 };
const LEVELS: readonly Level[] = [{ db: -3 }, { db: -12 }];
const BY_NAME: Readonly<Record<string, Level>> = { loud: { db: -3 }, soft: { db: -12 } };
const NAMED: ReadonlyMap<string, Level> = new Map([
  ["loud", { db: -3 }],
  ["soft", { db: -12 }],
]);
const ROWS: Level[][] = [[{ db: -3 }], [{ db: -12 }]];

function draw(level: Level | undefined): void {
  console.log(level?.db);
}

function show(rows: Level[][]): void {
  console.log(rows.length);
}

/** The first level, by its place. */
export function indexed(): void {
  const first = LEVELS[0];
  draw(first);
}

/** The first level, at its place. */
export function lookedUp(): void {
  const first = LEVELS.at(0);
  draw(first);
}

/** The level of a name, by its key. */
export function keyed(name: string): void {
  const level = BY_NAME[name];
  draw(level);
}

/** The level of a name, got by its key. */
export function got(name: string): void {
  const level = NAMED.get(name);
  draw(level);
}

/** The quiet level and a second gathered in a list, the first by its place. */
export function pushed(second: Level): void {
  const both = [QUIET];
  both.push(second);
  draw(both[0]);
}

/** The quiet level and a second gathered in a list, at its place. */
export function pushedLookedUp(second: Level): void {
  const both = [QUIET];
  both.push(second);
  draw(both.at(0));
}

/** A level added to the module's first row, by its place. */
export function added(level: Level): void {
  const row = ROWS[0];
  row.push(level);
  show(ROWS);
}

/** A level added to the module's first row, at its place. */
export function addedLookedUp(level: Level): void {
  const row = ROWS.at(0)!;
  row.push(level);
  show(ROWS);
}
