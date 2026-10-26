// The same drawing, where the function written in place returns what it gives back from a block.

export class Where {
  constructor(public place: string) {}
}

export class Touch {
  constructor(public op: string) {}

  drawn(): Where {
    return new Where(this.op);
  }
}

export function drawn(id: string, touches: Touch[]): [string, Where[]][] {
  return [[id, touches.map((t) => {
    return t.drawn();
  })]];
}
