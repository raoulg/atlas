import type { AxiosInstance } from "axios";

export class Api {
  constructor(readonly http: AxiosInstance) {}
}

function remove(api: Api, id: string): Promise<unknown> {
  return api.http.delete(`/items/${id}`);
}

function stamp(d: Date): number {
  d.setHours(0);
  return d.getTime();
}

export function main(api: Api): number {
  remove(api, "a");
  return stamp(new Date());
}
