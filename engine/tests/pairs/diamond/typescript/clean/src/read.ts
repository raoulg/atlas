import { Feed } from "./sources.js";

export async function headlines(url: string, cache: string): Promise<string[]> {
  const feed = new Feed(url, cache);
  return feed.lines();
}
