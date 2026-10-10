export function thema(): string {
  return localStorage.getItem("thema") ?? "licht";
}

export function bewaar(t: string): void {
  localStorage.setItem("thema", t);
  process.stdout.write(t);
}

export function luister(): WebSocket {
  return new WebSocket("wss://live.example.org/stream");
}
