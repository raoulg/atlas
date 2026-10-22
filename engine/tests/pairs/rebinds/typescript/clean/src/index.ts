// A module's body over the bases it names: each base is resolved, then the body is built. Every
// value has a name of its own.

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
  for (const base of bases) {
    resolved.push(resolve(base));
  }
  const body = new Body(name, resolved);
  return body;
}
