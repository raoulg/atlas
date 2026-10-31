/**
 * The levels of a signal, each looked up in the collection it is handed: by a subscript, or by the method that looks it
 * up. `pushed` and `pushedLookedUp` look up in a list they change in place first; `added` and `addedLookedUp` change the
 * row they look up, and so the rows.
 */

/** A level of a signal. */
export interface Level {
  db: number;
}

function draw(level: Level | undefined): void {
  console.log(level?.db);
}

function show(rows: Level[][]): void {
  console.log(rows.length);
}

/** The first level, by its place. */
export function indexed(levels: Level[]): void {
  const first = levels[0];
  draw(first);
}

/** The first level, at its place. */
export function lookedUp(levels: Level[]): void {
  const first = levels.at(0);
  draw(first);
}

/** The level of a name, by its key. */
export function keyed(levels: Record<string, Level>, name: string): void {
  const level = levels[name];
  draw(level);
}

/** The level of a name, got by its key. */
export function got(levels: Map<string, Level>, name: string): void {
  const level = levels.get(name);
  draw(level);
}

/** The first of two levels gathered in a list, by its place. */
export function pushed(first: Level, second: Level): void {
  const both = [first];
  both.push(second);
  draw(both[0]);
}

/** The first of two levels gathered in a list, at its place. */
export function pushedLookedUp(first: Level, second: Level): void {
  const both = [first];
  both.push(second);
  draw(both.at(0));
}

/** A level added to the first row, by its place. */
export function added(rows: Level[][], level: Level): void {
  const row = rows[0];
  row.push(level);
  show(rows);
}

/** A level added to the first row, at its place. */
export function addedLookedUp(rows: Level[][], level: Level): void {
  const row = rows.at(0)!;
  row.push(level);
  show(rows);
}
