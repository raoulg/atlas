// What fitting a curve gives: a fit, or why there is none.

export class Fit {
  constructor(readonly score: number) {}
}

export class Failed {
  constructor(readonly reason: string) {}
}
