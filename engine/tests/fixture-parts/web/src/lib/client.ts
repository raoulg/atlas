const BASE = "http://localhost:8000";

export interface Item {
  id: string;
}

export async function item(id: string): Promise<Item> {
  const response = await fetch(`${BASE}/api/items/${id}`);
  return response.json();
}

export async function order(id: string): Promise<void> {
  await fetch(BASE + "/v1/orders", { method: "POST", body: id });
}
