export class Event {
  constructor(readonly name: string) {}
}

export class Click extends Event {
  constructor(readonly x: number) {
    super("click");
  }
}

export interface Named {
  readonly name: string;
}

export class Handler {
  handle(e: Event): string {
    return e.name;
  }

  log(e: Click): string {
    return `${e.name} at ${e.x}`;
  }
}

export class ClickHandler extends Handler {
  override handle(e: Click): string {
    return `click at ${e.x}`;
  }

  override log(e: Named): string {
    return e.name;
  }
}
