/** Letters sorted onto a pile. */

export interface Letter {
  to: string;
}

export interface Pile {
  letters: Letter[];
  returned: Letter[];
  counts: number[];
}

/** How many letters a pile holds. */
function size(p: Pile): number {
  return p.letters.length;
}

/** A name as it is compared: trimmed, in lower case. */
function tidy(name: string): string {
  return name.trim().toLowerCase();
}

/** How many of the letters that came in are for `name`. */
export function addressed(incoming: Letter[], name: string): number {
  return incoming.filter((l) => tidy(l.to) === tidy(name)).length;
}

/** How many letters the piles hold together. */
export function total(piles: Pile[]): number {
  return piles.map(size).reduce((a, b) => a + b, 0);
}

/** Puts the letters for `name` on the pile, sends the others back, and notes how many came. */
export function sort(pile: Pile, name: string, incoming: Letter[]): void {
  const same = (s: string) => s.trim().toLowerCase();
  const mine: Letter[] = [];
  const others: Letter[] = [];
  for (const l of incoming) {
    if (same(l.to) === same(name)) {
      mine.push(l);
    } else {
      others.push(l);
    }
  }
  const came = mine.length;
  pile.letters.push(...mine);
  pile.returned.push(...others);
  pile.counts.push(came);
}
