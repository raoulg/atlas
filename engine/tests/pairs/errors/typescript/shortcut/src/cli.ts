// The total of the counts some lines hold: a line that holds no count throws the language's catch-all error, and the
// total swallows whatever is thrown, so the route never hears of it.

function parse(line: string): number {
  if (!/^\d+$/.test(line)) throw new Error("not a count: " + line);
  return Number(line);
}

function total(lines: string[]): number {
  try {
    let sum = 0;
    for (const line of lines) sum += parse(line);
    return sum;
  } catch {
    return 0;
  }
}

export function main(): void {
  console.log(total(["1", "2", "x"]));
}

main();
