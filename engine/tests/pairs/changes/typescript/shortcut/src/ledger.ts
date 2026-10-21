// A ledger of expenses: the functions change what they are handed and give nothing back.

export interface Expense {
  category: string;
}

export class Ledger {
  rows: Expense[] = [];

  takeFrom(other: Ledger): number {
    this.rows.push(...other.rows);
    other.rows.length = 0;
    return this.rows.length;
  }

  takeVictim(victim: Ledger): number {
    this.rows.push(...victim.rows);
    victim.rows.length = 0;
    return this.rows.length;
  }
}

function addLine(lines: string[], s: string): void {
  lines.push(s);
}

function count(m: Map<string, number>, k: string): void {
  m.set(k, (m.get(k) ?? 0) + 1);
}

function fileAll(expenses: Expense[]): void {
  for (const e of expenses) {
    e.category = "filed";
  }
}

function rename(e: Expense, name: string): number {
  const x = e;
  x.category = name;
  return 1;
}

export function main(): number {
  const ledger = new Ledger();
  const other = new Ledger();
  ledger.takeFrom(other);
  ledger.takeVictim(other);
  const lines: string[] = [];
  addLine(lines, "a");
  const m = new Map<string, number>();
  count(m, "a");
  fileAll(ledger.rows);
  return rename({ category: "a" }, "b") + lines.length + m.size;
}
