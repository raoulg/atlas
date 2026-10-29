export class Block {
  show(heading: string): void {
    console.log(heading);
  }
}

/** A block that opens a chapter: it shows as any block does. */
export class Section extends Block {}

export class Boxed extends Section {
  show(heading: string): void {
    if (!heading) {
      throw new Error("a box needs a heading");
    }
    console.log("[", heading, "]");
  }
}
