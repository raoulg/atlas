// The total of the counts some lines hold: a line that holds no count is an error of the program's own class, which
// each border hands on, up to the route.

export class NotACount extends Error {}

function parse(line: string): number {
  if (!/^\d+$/.test(line)) throw new NotACount(line);
  return Number(line);
}

function total(lines: string[]): number {
  let sum = 0;
  for (const line of lines) sum += parse(line);
  return sum;
}

export function main(): void {
  console.log(total(["1", "2", "x"]));
}

main();
