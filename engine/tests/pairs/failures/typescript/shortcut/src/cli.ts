// Settings read from one of several sources, chosen at run time. Each source fails its own way, with whatever error
// came to hand, so the route that loads through the base can fail in as many ways as there are sources.

abstract class Source {
  abstract load(): Record<string, string>;
}

class FileSource extends Source {
  load(): Record<string, string> {
    throw new Error("no settings file");
  }
}

class EnvSource extends Source {
  load(): Record<string, string> {
    throw new TypeError("no settings variable");
  }
}

function settings(source: Source): Record<string, string> {
  return source.load();
}

export function main(): void {
  console.log(settings(new FileSource()), new EnvSource());
}

main();
