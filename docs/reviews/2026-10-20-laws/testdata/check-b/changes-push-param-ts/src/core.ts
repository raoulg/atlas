function addLine(lines: string[], s: string): number {
  lines.push(s);
  return lines.length;
}

function count(m: Map<string, number>, k: string): number {
  m.set(k, 1);
  return m.size;
}

function localCopy(xs: number[]): number[] {
  const ys = [...xs];
  ys.push(1);
  return ys;
}

export function main(): number {
  return addLine([], "a") + count(new Map(), "a") + localCopy([1]).length;
}
