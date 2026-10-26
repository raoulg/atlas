export interface Fit {
  kind: "fit";
  score: number;
}

export interface Failed {
  kind: "failed";
  reason: string;
}

export type Result = Fit | Failed;
