"""A server that greets each client by the name it sends, and lets the client read the greeting
before the connection closes."""

import socket
import time

ADDRESS = ("127.0.0.1", 7878)
"""Where the server listens."""


def main() -> None:
    with socket.create_server(ADDRESS) as listener:
        while True:
            conn, _ = listener.accept()
            with conn:
                answer(conn)


def answer(conn: socket.socket) -> None:
    """Read the client's name and write its greeting back, then let the client read it."""
    name = conn.recv(256)
    conn.sendall(greeting(name).encode())
    linger()


def greeting(name: bytes) -> str:
    """The greeting for a name."""
    return f"hello, {name.decode(errors='replace').strip()}\n"


def linger() -> None:
    """Wait a moment, so that the client has read the greeting before the connection is dropped."""
    time.sleep(1)
