export interface Expense {
  category: string;
}

function fileAll(expenses: Expense[]): void {
  for (const e of expenses) {
    e.category = e.category.trim();
  }
}

function fileAllCounted(expenses: Expense[]): number {
  for (const e of expenses) {
    e.category = e.category.trim();
  }
  return expenses.length;
}

export function main(): number {
  const xs: Expense[] = [{ category: " a " }];
  fileAll(xs);
  return fileAllCounted(xs);
}
