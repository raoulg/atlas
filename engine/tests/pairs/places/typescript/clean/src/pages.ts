// Pages made from the template that ships with the package.

import { readFileSync } from "node:fs";

export function page(title: string): string {
  const template = readFileSync(new URL("./template.html", import.meta.url), "utf8");
  return template.replace("{title}", title);
}
