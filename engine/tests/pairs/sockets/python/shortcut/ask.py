"""A client of the greeter: sends a name and prints the greeting it reads back."""

import socket

ADDRESS = ("127.0.0.1", 7878)
"""Where the greeter listens."""


def main() -> None:
    with socket.create_connection(ADDRESS) as conn:
        conn.sendall(b"ada")
        conn.shutdown(socket.SHUT_WR)
        print(hear(conn))


def hear(conn: socket.socket) -> str:
    """The greeting the server writes back, read to its end."""
    chunks = []
    while chunk := conn.recv(1024):
        chunks.append(chunk)
    return b"".join(chunks).decode()
