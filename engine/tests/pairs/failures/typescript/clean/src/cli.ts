// Settings read from one of several sources, chosen at run time. Every source fails as the base says a source fails,
// with the program's own error, so the route that loads through the base hears of one error.

export class SourceError extends Error {}

abstract class Source {
  abstract load(): Record<string, string>;
}

class FileSource extends Source {
  load(): Record<string, string> {
    throw new SourceError("no settings file");
  }
}

class EnvSource extends Source {
  load(): Record<string, string> {
    throw new SourceError("no settings variable");
  }
}

function settings(source: Source): Record<string, string> {
  return source.load();
}

export function main(): void {
  console.log(settings(new FileSource()), new EnvSource());
}

main();
