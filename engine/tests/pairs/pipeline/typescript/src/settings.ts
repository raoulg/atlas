/** The choices of a run that are not places: how far to smooth and how the figure looks. */
export interface Settings {
  /** How many months the smoothed mean is taken over, centred on each. */
  readonly months: number;
  readonly title: string;
  /** The figure's size, in pixels. */
  readonly width: number;
  readonly height: number;
}

export const DEFAULT_SETTINGS: Settings = { months: 13, title: "Monthly mean total sunspot number", width: 1000, height: 400 };
