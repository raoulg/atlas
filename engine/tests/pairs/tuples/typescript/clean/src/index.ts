// The mean of each station's readings. A station's mean is a record with a name of its own, and the
// station and its store come back together, where there is one.

/** A weather station. */
export interface Station {
  readonly id: string;
  readonly readings: readonly number[];
}

/** The mean of a station's readings. */
export interface Mean {
  readonly station: Station;
  readonly mean: number;
}

/** Where the stations are kept. */
export interface Store {
  readonly stations: readonly Station[];
}

/** The mean of each station's readings. */
export function means(stations: readonly Station[]): Mean[] {
  return stations.map((station) => ({ station, mean: station.readings.reduce((a, b) => a + b, 0) / station.readings.length }));
}

/** The first station of a store, with the store itself, where it keeps one. */
export function first(store: Store): [Station, Store] | undefined {
  const station = store.stations[0];
  return station === undefined ? undefined : [station, store];
}
