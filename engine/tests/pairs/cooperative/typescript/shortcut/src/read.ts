import { Feed } from "./sources.js";
import { Store } from "./store.js";

export async function headlines(url: string, cache: string): Promise<string[]> {
  const feed = new Feed(url, new Store(cache));
  const text = await feed.read();
  return text.split("\n");
}
