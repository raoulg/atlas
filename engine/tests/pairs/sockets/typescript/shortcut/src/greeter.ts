// A server that greets each client by the name it sends, and lets the client read the greeting
// before the connection closes.
import * as net from "node:net";

/** Where the server listens. */
const PORT = 7878;
const HOST = "127.0.0.1";

function main(): void {
  const listener = net.createServer((conn) => answer(conn));
  listener.listen(PORT, HOST);
}

/** Read the client's name and write its greeting back, then let the client read it. */
function answer(conn: net.Socket): void {
  conn.once("data", async (name: Buffer) => {
    conn.write(greeting(name));
    await linger();
    conn.destroy();
  });
}

/** The greeting for a name. */
function greeting(name: Buffer): string {
  return `hello, ${name.toString().trim()}\n`;
}

/** Wait a moment, so that the client has read the greeting before the connection is dropped. */
async function linger(): Promise<void> {
  await new Promise((done) => setTimeout(done, 1000));
}

main();
