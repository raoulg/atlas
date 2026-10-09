import { createServerFn } from "@tanstack/react-start";

import type { Regel, Uitkomst } from "./model";

export async function laad(id: string): Promise<Regel[]> {
  const antwoord = await fetch(`https://api.example.org/regels/${id}`);
  console.log("geladen");
  return antwoord.json();
}

export function kies(uitkomst: Uitkomst, modus: string): string {
  if (modus === "kort") return "kort";
  return modus === "kort" ? "k" : "lang";
}

export const zoek = createServerFn({ method: "GET" }).handler(async ({ data }): Promise<Uitkomst> => {
  await laad(data);
  return { reden: "geen" };
});
