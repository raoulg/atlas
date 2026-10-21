import { Store } from "./store.js";

export class Source {
  constructor(readonly url: string) {}

  async read(): Promise<string> {
    const response = await fetch(this.url);
    return response.text();
  }
}

export class Buffered extends Source {
  async lines(): Promise<string[]> {
    return (await this.read()).split("\n");
  }
}

export class Cached extends Buffered {
  readonly store: Store;

  constructor(url: string, cache: string) {
    super(url);
    this.store = new Store(cache);
  }

  override async read(): Promise<string> {
    return this.store.get(this.url);
  }
}

export class Feed extends Cached {}
