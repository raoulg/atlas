let total = 0;

class Counter {
  static total = 0;
}

function bumpModule(): void {
  total += 1;
}

function bumpStatic(): void {
  Counter.total += 1;
}

function stop(): void {
  process.exit(1);
}

function fail(): void {
  throw new Error("stop");
}

export function main(): number {
  bumpModule();
  bumpStatic();
  stop();
  fail();
  return total + Counter.total;
}
