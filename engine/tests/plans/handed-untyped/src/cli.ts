// Two values of a type the program does not define, each made by a call, handed by name to a function that hands them
// on by name to calls of its own.

import type { Session } from "sessions";

function describe(session: Session | null): string {
  return session ? String(session) : "";
}

function notices(before: Session | null, after: Session | null): string[] {
  return [describe(before), describe(after)];
}

const before = JSON.parse("{}");
const after = JSON.parse("{}");
console.log(notices(before, after));
