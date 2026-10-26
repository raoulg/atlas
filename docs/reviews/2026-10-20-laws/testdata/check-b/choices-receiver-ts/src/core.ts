class Runner {
  constructor(readonly mode: string) {}

  size(): number {
    const a = this.mode === "parallel" ? 64 : 1;
    return a + (this.mode === "parallel" ? 8 : 1);
  }
}

function size(mode: string): number {
  const a = mode === "parallel" ? 64 : 1;
  return a + (mode === "parallel" ? 8 : 1);
}

export function main(mode: string): number {
  return new Runner(mode).size() + size(mode);
}
