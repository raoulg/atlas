import { Store } from "./store.js";

export class Source {
  constructor(readonly url: string) {}

  async read(): Promise<string> {
    const response = await fetch(this.url);
    return response.text();
  }
}

export class Cached extends Source {
  constructor(
    url: string,
    readonly store: Store,
  ) {
    super(url);
  }

  async read(): Promise<string> {
    if (this.store.has(this.url)) {
      return this.store.get(this.url);
    }
    const text = await super.read();
    this.store.put(this.url, text);
    return text;
  }
}

export class Feed extends Cached {
  async read(): Promise<string> {
    const text: string = await Source.prototype.read.call(this);
    return text.trim();
  }
}
