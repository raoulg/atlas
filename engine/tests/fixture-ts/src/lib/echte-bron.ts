import type { BronLike } from "./bron";

export class EchteBron implements BronLike {
  async lees(id: string): Promise<unknown> {
    return fetch(`https://bron.example.org/${id}`);
  }
}
