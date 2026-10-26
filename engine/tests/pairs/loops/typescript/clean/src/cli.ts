// Each station of a network, reported: the stations come from the network the route is handed.
import { readFileSync } from "node:fs";

export class Station {
  constructor(public name: string) {}
}

export class Network {
  stations: Station[] = [];
}

function load(path: string): Network {
  const network = new Network();
  for (const line of readFileSync(path, "utf8").split("\n")) network.stations.push(new Station(line));
  return network;
}

function report(station: Station): void {
  console.log(station.name);
}

function reportAll(network: Network): void {
  for (const station of network.stations) {
    report(station);
  }
}

export function run(): void {
  const network = load("stations.txt");
  reportAll(network);
}

run();
