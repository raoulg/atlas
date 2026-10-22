// A module's body over the bases it names: each base is resolved, then the body is built. One name,
// `b`, is bound twice: to each base in the loop, then to the body.

/** A module's name and the bases it resolves to. */
export class Body {
  constructor(
    readonly name: string,
    readonly bases: string[],
  ) {}
}

/** The name a base is known by. */
function resolve(base: string): string {
  return base.trim().toLowerCase();
}

/** The body of the module `name` over its `bases`, each resolved. */
export function link(name: string, bases: string[]): Body {
  const resolved: string[] = [];
  for (const b of bases) {
    resolved.push(resolve(b));
  }
  const b = new Body(name, resolved);
  return b;
}
