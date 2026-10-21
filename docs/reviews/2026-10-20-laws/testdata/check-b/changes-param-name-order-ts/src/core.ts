export class Graph {
  name = "";

  stealOther(other: Graph): number {
    this.name = other.name;
    other.name = "";
    return 1;
  }

  stealVictim(victim: Graph): number {
    this.name = victim.name;
    victim.name = "";
    return 1;
  }
}

export function main(): number {
  const g = new Graph();
  return g.stealOther(new Graph()) + g.stealVictim(new Graph());
}
