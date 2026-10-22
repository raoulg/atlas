// The same client, with each path and its method behind a key looked up at run time, and an
// environment variable that may stand in for the path.
type Endpoint = "item" | "order";

const DEFAULT_PATHS: Record<string, string> = {
  ITEM_PATH: "/api/items",
  ORDER_PATH: "/api/orders",
};

const ENDPOINTS: { key: Endpoint; env: string; method: string }[] = [
  { key: "item", env: "ITEM_PATH", method: "GET" },
  { key: "order", env: "ORDER_PATH", method: "POST" },
];

function endpoint(key: Endpoint): string {
  const found = ENDPOINTS.find((e) => e.key === key);
  const path = process.env[found!.env] ?? DEFAULT_PATHS[found!.env];
  return `${process.env.API_BASE_URL}${path}`;
}

function method(key: Endpoint): string {
  return ENDPOINTS.find((e) => e.key === key)!.method;
}

export interface Item {
  id: string;
}

export async function item(id: string): Promise<Item> {
  const response = await fetch(`${endpoint("item")}/${id}`);
  return response.json();
}

export async function order(id: string): Promise<void> {
  await fetch(endpoint("order"), { method: method("order"), body: id });
}
