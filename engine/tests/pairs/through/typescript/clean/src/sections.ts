export class Section {
  show(heading: string): void {
    console.log(heading);
  }
}

export class Boxed extends Section {
  show(heading: string): void {
    if (!heading) {
      throw new Error("a box needs a heading");
    }
    console.log("[", heading, "]");
  }
}
