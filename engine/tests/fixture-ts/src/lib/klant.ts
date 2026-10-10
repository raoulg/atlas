const BASIS = "/api";

async function vraag(methode: string, pad: string): Promise<Response> {
  return fetch(`${BASIS}${pad}`, { method: methode });
}

const api = {
  haal: (pad: string) => vraag("GET", pad),
  stuur: (pad: string) => vraag("POST", pad),
};

export function regel(id: string): Promise<Response> {
  return api.haal(`/regels/${id}`);
}

export function nieuw(): Promise<Response> {
  return api.stuur("/regels");
}
