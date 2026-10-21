export class Fit {
  constructor(readonly score: number) {}
}

export class Failed {
  constructor(readonly reason: string) {}
}

export type Result = Fit | Failed;
