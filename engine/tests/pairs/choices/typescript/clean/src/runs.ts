// A job run in one of two modes, and a line that says how it went.

export enum Mode {
  Serial = "serial",
  Parallel = "parallel",
}

/** What a mode means for a run. */
interface Plan {
  batch: number;
  workers: number;
  retries: number;
  timeout: number;
}

const PLANS: Record<Mode, Plan> = {
  [Mode.Serial]: { batch: 1, workers: 1, retries: 1, timeout: 600 },
  [Mode.Parallel]: { batch: 64, workers: 8, retries: 3, timeout: 60 },
};

export enum Level {
  Quiet = "quiet",
  Loud = "loud",
}

/** How a level marks a line. */
interface Marks {
  prefix: string;
  suffix: string;
}

const MARKS: Record<Level, Marks> = {
  [Level.Quiet]: { prefix: "", suffix: "." },
  [Level.Loud]: { prefix: "!", suffix: "!" },
};

export interface Settings {
  mode: Mode;
  level: Level;
}

class Job {
  constructor(
    readonly name: string,
    readonly steps: string[],
  ) {}
}

class Message {
  constructor(readonly text: string) {}
}

export class Runner {
  readonly plan: Plan;

  constructor(mode: Mode) {
    this.plan = PLANS[mode];
  }

  batch(): number {
    return this.plan.batch;
  }

  workers(): number {
    return this.plan.workers;
  }
}

export class Report {
  constructor(readonly level: Level) {}

  line(text: string): string {
    const marks = MARKS[this.level];
    return marks.prefix + text + marks.suffix;
  }
}

function load(name: string): Job {
  return new Job(name, [name]);
}

function infer(name: string): Job {
  return new Job(name, []);
}

/** Where the job comes from: the cache, or what its name says. */
function source(name: string, cached: boolean): Job {
  return cached ? load(name) : infer(name);
}

function finished(job: Job, mode: Mode): boolean {
  return job.steps.length * new Runner(mode).batch() <= PLANS[mode].timeout;
}

function send(message: Message): number {
  console.log(message.text);
  return message.text.length;
}

function report(job: Job, level: Level, done: boolean): number {
  const message = done ? new Message(new Report(level).line(`${job.name} done`)) : new Message(`${job.name} failed`);
  return send(message);
}

export function run(name: string, settings: Settings, cached: boolean): number {
  const job = source(name, cached);
  return report(job, settings.level, finished(job, settings.mode)) + PLANS[settings.mode].retries;
}
