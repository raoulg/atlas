/** A rule on the map. */
export interface Regel {
  id: string;
  laag: Laag;
}

export interface Treffer {
  regel: Regel;
}

export interface Mis {
  reden: string;
}

export type Uitkomst = Treffer | Mis;

export class Bron {
  lees(): Regel[] {
    return [];
  }
}

export class Laag {
  constructor(private bron: Bron) {}

  toon(): Regel[] {
    return this.bron.lees();
  }
}
