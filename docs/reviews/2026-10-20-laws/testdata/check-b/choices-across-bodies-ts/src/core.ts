class Runner {
  constructor(readonly mode: string) {}
  batch(): number {
    return this.mode === "parallel" ? 64 : 1;
  }
  workers(): number {
    return this.mode === "parallel" ? 8 : 1;
  }
}

function batch(mode: string): number {
  return mode === "parallel" ? 64 : 1;
}

function workers(mode: string): number {
  return mode === "parallel" ? 8 : 1;
}

function twice(mode: string): number {
  const b = mode === "parallel" ? 64 : 1;
  return b + (mode === "parallel" ? 8 : 1);
}

export function main(mode: string): number {
  const r = new Runner(mode);
  return r.batch() + r.workers() + batch(mode) + workers(mode) + twice(mode);
}
