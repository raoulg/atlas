// The same chart, which binds its title and its size to names before it hands them on.

export class Size {
  constructor(public width: number) {}
}

export class Chart {
  constructor(public title: string, public size: Size) {}

  take(title: string, width: number): void {
    this.title = title;
    this.size.width = width;
  }

  lend(other: Chart): void {
    const t = this.title;
    const s = this.size;
    other.take(t, s.width);
  }
}
