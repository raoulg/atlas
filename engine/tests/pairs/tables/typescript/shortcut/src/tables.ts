// How long a run may take in its mode: the modes in one list, their plans in another at the same places.

export enum Mode {
  Serial = "serial",
  Parallel = "parallel",
}

export interface Plan {
  workers: number;
  timeout: number;
}

function index(modes: Mode[], mode: Mode): number {
  return modes.indexOf(mode);
}

export function timeout(modes: Mode[], plans: Plan[], mode: Mode): number {
  return plans[index(modes, mode)].timeout;
}
