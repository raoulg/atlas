import type { Regel } from "./model";

export class Teller {
  aantal = 0;

  constructor(start: number) {
    this.aantal = start;
  }

  tel(): void {
    this.aantal++;
  }
}

export function verzamel(regels: Regel[], uit: Regel[]): void {
  for (const r of regels) uit.push(r);
}

export function hernoem(regel: Regel, id: string): void {
  regel.id = id;
}

export function telAlles(regels: Regel[]): number {
  return regels.length;
}
