// The report takes the Shelf the store makes and prints it.
import type { Shelf } from "../store/disk.js";

export function render(shelf: Shelf): void {
  for (const title of shelf.titles) {
    console.log(title);
  }
}
