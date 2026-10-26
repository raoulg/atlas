// A job run in one of two modes, and a line that says how it went.

export enum Level {
  Quiet = "quiet",
  Loud = "loud",
}

export interface Settings {
  mode: string;
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
  constructor(readonly mode: string) {}

  batch(): number {
    return this.mode === "parallel" ? 64 : 1;
  }

  workers(): number {
    return this.mode === "parallel" ? 8 : 1;
  }
}

export class Report {
  constructor(readonly level: string) {}

  line(text: string): string {
    const l = this.level;
    return (l === "loud" ? "!" : "") + text + (l === "loud" ? "!" : ".");
  }
}

function retries(mode: string): number {
  return mode === "parallel" ? 3 : 1;
}

function timeout(mode: string): number {
  return mode === "parallel" ? 60 : 600;
}

function prefix(level: Level): string {
  return level === Level.Loud ? "!" : "";
}

function suffix(level: Level): string {
  switch (level) {
    case Level.Loud:
      return "!";
    default:
      return ".";
  }
}

function load(name: string): Job {
  return new Job(name, [name]);
}

function infer(name: string): Job {
  return new Job(name, []);
}

function finished(job: Job, mode: string): boolean {
  return job.steps.length * new Runner(mode).batch() <= timeout(mode);
}

function send(message: Message): number {
  console.log(message.text);
  return message.text.length;
}

export function run(name: string, settings: Settings, cached: boolean): number {
  const job = cached ? load(name) : infer(name);
  const done = finished(job, settings.mode);
  const text = prefix(settings.level) + job.name + (done ? " done" : " failed") + suffix(settings.level);
  return send(new Message(text)) + retries(settings.mode);
}
