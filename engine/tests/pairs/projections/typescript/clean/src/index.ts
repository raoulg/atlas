// A chart lends another its title and its width, handing each as a field of its own.

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
    other.take(this.title, this.size.width);
  }
}
