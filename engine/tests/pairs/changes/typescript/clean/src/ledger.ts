// A ledger of expenses: a value changes through a method of its own, or a new one is handed back.
import type { AxiosInstance } from "axios";

export interface Expense {
  category: string;
}

export class Api {
  constructor(readonly http: AxiosInstance) {}
}

export class Ledger {
  rows: Expense[] = [];

  add(e: Expense): number {
    this.rows.push(e);
    return this.rows.length;
  }

  merged(other: Ledger): Ledger {
    const out = new Ledger();
    out.rows = [...this.rows, ...other.rows];
    return out;
  }
}

function withLine(lines: readonly string[], s: string): string[] {
  return [...lines, s];
}

function counted(m: ReadonlyMap<string, number>, k: string): Map<string, number> {
  const out = new Map(m);
  out.set(k, (out.get(k) ?? 0) + 1);
  return out;
}

function refiled(e: Expense): Expense {
  e = { ...e };
  e.category = "filed";
  return e;
}

function remove(api: Api, id: string): Promise<unknown> {
  return api.http.delete(`/items/${id}`);
}

export function main(api: Api): number {
  const ledger = new Ledger();
  ledger.add(refiled({ category: "a" }));
  void remove(api, "a");
  return ledger.merged(new Ledger()).rows.length + withLine([], "a").length + counted(new Map(), "a").size;
}
