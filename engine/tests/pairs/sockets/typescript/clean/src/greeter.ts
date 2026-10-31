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
  conn.once("data", (name: Buffer) => {
    conn.write(greeting(name));
    linger(conn);
  });
}

/** The greeting for a name. */
function greeting(name: Buffer): string {
  return `hello, ${name.toString().trim()}\n`;
}

/** Close the writing half, so that the client reads the end of the greeting, and read what it
 * still sends for a moment before the connection is let go. */
function linger(conn: net.Socket): void {
  conn.end();
  conn.setTimeout(1000, () => conn.destroy());
  conn.on("data", () => {});
}

main();
