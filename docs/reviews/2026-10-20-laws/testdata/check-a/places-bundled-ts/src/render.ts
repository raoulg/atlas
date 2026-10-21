import { readFileSync } from "node:fs";

export function render(title: string): string {
  const template = readFileSync(new URL("./template.html", import.meta.url), "utf8");
  return template.replace("{title}", title);
}
