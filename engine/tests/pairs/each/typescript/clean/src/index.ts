// What a node draws: its places, by the node's id, each touch drawn by a function written in place that gives it back.

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
  return [[id, touches.map((t) => t.drawn())]];
}
