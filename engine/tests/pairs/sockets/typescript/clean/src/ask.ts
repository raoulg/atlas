// A client of the greeter: sends a name and prints the greeting it reads back.
import * as net from "node:net";

/** Where the greeter listens. */
const PORT = 7878;
const HOST = "127.0.0.1";

function main(): void {
  const conn = net.createConnection(PORT, HOST);
  conn.end("ada");
  hear(conn);
}

/** The greeting the server writes back, read to its end and printed. */
function hear(conn: net.Socket): void {
  const chunks: Buffer[] = [];
  conn.on("data", (chunk: Buffer) => chunks.push(chunk));
  conn.on("end", () => console.log(Buffer.concat(chunks).toString()));
}

main();
