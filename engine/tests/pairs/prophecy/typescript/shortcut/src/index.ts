/** Writes reports and gives nothing back. */
export class Sink {
  write(report: string): void {
    console.log(report);
  }
}

/**
 * What goes in comes out.
 */
export declare class Store {
  write(report: string): void;
  read(): string;
}
