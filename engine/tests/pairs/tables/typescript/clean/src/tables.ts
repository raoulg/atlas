// How long a run may take in its mode: one table of plans, a Plan by its Mode.

export enum Mode {
  Serial = "serial",
  Parallel = "parallel",
}

export interface Plan {
  workers: number;
  timeout: number;
}

export function timeout(plans: Record<Mode, Plan>, mode: Mode): number {
  return plans[mode].timeout;
}
