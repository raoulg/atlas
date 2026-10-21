class Job {
  x = 1;
  constructor(private readonly onDone: () => void) {}

  reset(): void {
    this.x = 0;
  }

  finish(): void {
    this.onDone();
  }
}

function run(onDone: () => void): void {
  onDone();
}

export function main(): number {
  const job = new Job(() => console.log("done"));
  job.reset();
  job.finish();
  run(() => console.log("ran"));
  return job.x;
}
