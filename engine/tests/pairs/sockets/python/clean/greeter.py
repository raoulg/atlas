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
    linger(conn)


def greeting(name: bytes) -> str:
    """The greeting for a name."""
    return f"hello, {name.decode(errors='replace').strip()}\n"


def linger(conn: socket.socket) -> None:
    """Close the writing half, so that the client reads the end of the greeting, and read what it
    still sends for a moment before the connection is let go."""
    conn.shutdown(socket.SHUT_WR)
    conn.settimeout(0.1)
    until = time.monotonic() + 1
    while time.monotonic() < until:
        try:
            if not conn.recv(1024):
                break
        except OSError:
            break
