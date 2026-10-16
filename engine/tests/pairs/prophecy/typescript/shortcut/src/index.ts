/** Writes reports and gives nothing back. */
export class Sink {
  write(report: string): void {
    console.log(report);
  }
}

/**
 * What goes in comes out.
 * @prophecy supersedes=Sink
 */
export declare class Store {
  constructor();
  write(report: string): void;
  read(): string;
}

export function run(report: string): void {
  new Store().write(report);
}

