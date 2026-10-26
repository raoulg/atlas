export interface Order {
  reference: string;
  weight: number;
}

export class Label {
  constructor(readonly text: string) {}
}
