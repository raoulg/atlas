// A reading shown in the unit its kind names. The unit is chosen in place: a name declared first and
// set in each arm of an `if`, with no step to say where it comes from.

/** A unit of temperature, by the letter it is shown with. */
export interface Unit {
  readonly letter: string;
}

/** A value read, in its unit. */
export interface Reading {
  readonly value: number;
  readonly unit: Unit;
}

export const CELSIUS: Unit = { letter: "C" };
export const KELVIN: Unit = { letter: "K" };

/** A reading as a line of text. */
function label(reading: Reading): string {
  return `${reading.value} ${reading.unit.letter}`;
}

/** The value `value` of kind `kind`, as a line of text. */
export function show(value: number, kind: string): string {
  let unit;
  if (kind === "k") {
    unit = KELVIN;
  } else {
    unit = CELSIUS;
  }
  const reading: Reading = { value, unit };
  return label(reading);
}
