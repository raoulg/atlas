// Each station of a network, reported: the stations are the module's own list, not the network's.
export class Station {
  constructor(public name: string) {}
}

const STATIONS: Station[] = [new Station("De Bilt"), new Station("Eelde")];

function report(station: Station): void {
  console.log(station.name);
}

function reportAll(): void {
  for (const station of STATIONS) {
    report(station);
  }
}

export function run(): void {
  reportAll();
}

run();
