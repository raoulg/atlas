// A reading shown in the unit its kind names. The unit is what a function gives, so the step that
// chooses it is a step of its own, and the reading is built from what it gives.

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

/** The unit a kind of reading names: `k` for Kelvin, anything else for Celsius. */
function unitOf(kind: string): Unit {
  return kind === "k" ? KELVIN : CELSIUS;
}

/** A reading as a line of text. */
function label(reading: Reading): string {
  return `${reading.value} ${reading.unit.letter}`;
}

/** The value `value` of kind `kind`, as a line of text. */
export function show(value: number, kind: string): string {
  const unit = unitOf(kind);
  const reading: Reading = { value, unit };
  return label(reading);
}
