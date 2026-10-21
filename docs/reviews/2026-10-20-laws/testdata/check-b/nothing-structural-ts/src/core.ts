export interface Task {
  run(): void;
}

class Echo {
  run(): void {
    console.log("echo");
  }
}

function runTask(t: Task): void {
  t.run();
}

export function main(): number {
  runTask(new Echo());
  return 0;
}
