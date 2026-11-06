export class Report {
  constructor(
    readonly title: string,
    readonly rows: string[],
  ) {}
}

export class Receipt {
  constructor(readonly path: string) {}
}
