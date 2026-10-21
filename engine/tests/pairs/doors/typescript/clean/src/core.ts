/** A length in metres. */
export class Length {
  private m: number;

  constructor(meters: number) {
    this.m = meters;
  }

  get meters(): number {
    return this.m;
  }

  set meters(value) {
    this.m = value;
  }

  plus(other: Length): Length {
    return new Length(this.m + other.meters);
  }
}

export function scale(x: number, k: number): number;
export function scale(x: bigint, k: bigint): bigint;
export function scale(x: any, k: any): any {
  return x * k;
}

export function log(...parts: string[]): void {
  console.log(...parts);
}

export function parse(text: unknown): Length {
  return new Length(Number(String(text).replace(/m$/, "")));
}
