import { post } from "../../ledger/post.js";

export function postedTwice(n: number): number {
  return post(post(n));
}
