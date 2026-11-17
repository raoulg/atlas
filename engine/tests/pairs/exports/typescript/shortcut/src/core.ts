export function scale(x: number, k: number): number {
  return x * k;
}

export function metres(text: string): number {
  return Number(text.replace(/m$/, ""));
}
