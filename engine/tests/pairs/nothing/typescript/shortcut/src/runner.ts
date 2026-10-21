// The same job runner, with what its steps leave out of sight of a reader.

export interface Task {
  run(): void;
}

class Echo {
  run(): void {
    console.log("echo");
  }
}

function runTask(task: Task): void {
  task.run();
}

class Job {
  constructor(private readonly onDone: () => void) {}

  finish(): void {
    this.onDone();
  }
}

class Counter {
  static runs = 0;
}

function count(): void {
  Counter.runs += 1;
}

export async function settle(): Promise<void> {}

function stop(): void {
  throw new Error("finished");
}

export async function main(): Promise<number> {
  runTask(new Echo());
  new Job(() => console.log("done")).finish();
  count();
  await settle();
  stop();
  return Counter.runs;
}
