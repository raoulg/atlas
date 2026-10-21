// Each step of a job leaves something a reader sees: a place, a value it is handed, the end of the run.
import { writeFile } from "node:fs/promises";

export interface Task {
  run(): void;
}

class Echo implements Task {
  run(): void {
    console.log("echo");
  }
}

function runTask(task: Task): void {
  task.run();
}

function finish(onDone: () => void): void {
  onDone();
}

let runs = 0;

function count(): void {
  runs += 1;
}

export async function settle(log: string): Promise<void> {
  await writeFile(log, "settled\n");
}

function stop(): void {
  process.exit(1);
}

export async function main(log: string): Promise<number> {
  runTask(new Echo());
  finish(() => console.log("done"));
  count();
  await settle(log);
  stop();
  return runs;
}
