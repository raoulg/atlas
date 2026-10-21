export interface Shape {
  area(): number;
}

export class Circle implements Shape {
  constructor(readonly r: number) {}

  area(): number {
    return 3.14 * this.r * this.r;
  }
}

export class Factory {
  make(): Shape {
    return { area: () => 0 };
  }
}

export class CircleFactory extends Factory {
  make(): Circle {
    return new Circle(1);
  }
}
