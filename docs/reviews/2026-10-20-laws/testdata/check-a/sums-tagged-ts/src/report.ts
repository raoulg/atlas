import type { Result } from "./model.js";

export function report(r: Result): string {
  switch (r.kind) {
    case "fit":
      return `fit ${r.score}`;
    case "failed":
      return `failed: ${r.reason}`;
  }
}
