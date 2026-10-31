// The levels of a signal, gathered into an array the body makes by its own code and changed after it binds it, in one arm
// of a choice or in a loop: the step after the change shows what the choice or the loop hands on.

/** A level of a signal. */
export interface Level {
  readonly db: number;
}

function show(levels: Level[]): void {
  console.log(levels);
}

/** The level, shown where it is loud: an empty array, pushed onto in one arm. */
export function picked(level: Level, loud: boolean): void {
  const both: Level[] = [];
  if (loud) {
    both.push(level);
  }
  show(both);
}

/** The first level, and the second beside it where it is loud: an array of the first, pushed onto in one arm. */
export function gathered(first: Level, second: Level, loud: boolean): void {
  const both: Level[] = [first];
  if (loud) {
    both.push(second);
  }
  show(both);
}

/** Every level, shown: an empty array, pushed onto in a loop. */
export function looped(levels: Level[]): void {
  const every: Level[] = [];
  for (const level of levels) {
    every.push(level);
  }
  show(every);
}
