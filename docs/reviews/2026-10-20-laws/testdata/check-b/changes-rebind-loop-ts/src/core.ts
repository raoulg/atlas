export interface Expense {
  category: string;
}

function refiled(e: Expense): Expense {
  e = { ...e };
  e.category = "x";
  return e;
}

function fileAll(expenses: Expense[]): number {
  for (const e of expenses) {
    e.category = "x";
  }
  return expenses.length;
}

function fileOne(e: Expense): number {
  e.category = "x";
  return 1;
}

export function main(): number {
  const e: Expense = { category: "a" };
  return refiled(e).category.length + fileAll([e]) + fileOne(e);
}
