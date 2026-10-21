export class Event {
  constructor(readonly name: string) {}
}

export class Click extends Event {
  constructor(readonly x: number) {
    super("click");
  }
}

export class Handler {
  handle(e: Event): string {
    return e.name;
  }
}

export class ClickHandler extends Handler {
  override handle(e: Click): string {
    return `click at ${e.x}`;
  }
}
