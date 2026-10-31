// The page asks the server it is served by, at the path of its notes: a GET for what they hold,
// a POST to keep one.

export async function notes(): Promise<string[]> {
  const response = await fetch("/api/notes");
  return response.json();
}

export async function keep(note: string): Promise<void> {
  await fetch("/api/notes", { method: "POST", body: note });
}
