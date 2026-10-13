// The backend's routes, asked for by the path they are served at; only the host comes from
// the environment.
const BASE = process.env.API_BASE_URL ?? "http://localhost:8000";

function api(path: string): string {
  return `${BASE}${path}`;
}

export interface Item {
  id: string;
}

export async function item(id: string): Promise<Item> {
  const response = await fetch(api(`/api/items/${id}`));
  return response.json();
}

export async function order(id: string): Promise<void> {
  await fetch(api("/api/orders"), { method: "POST", body: id });
}
