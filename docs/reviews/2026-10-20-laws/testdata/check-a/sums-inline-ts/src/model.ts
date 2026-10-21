export class Fit {
  constructor(readonly score: number) {}
}

export class Failed {
  constructor(readonly reason: string) {}
}
