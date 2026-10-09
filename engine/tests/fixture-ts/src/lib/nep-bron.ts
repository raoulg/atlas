import type { BronLike } from "./bron";

export class NepBron implements BronLike {
  async lees(id: string): Promise<unknown> {
    return id;
  }
}
