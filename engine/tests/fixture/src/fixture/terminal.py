"""The terminal both ways: what the human types, and what is shown."""

import curses
import getpass
import sys


def ask_name() -> str:
    return input("name? ")


def ask_secret() -> str:
    return getpass.getpass()


def first_line() -> str:
    return sys.stdin.readline()


def keep(path: str) -> None:
    with open(path, "w") as f:
        print("kept", file=f)


def draw(window: curses.window) -> int:
    window.addstr(0, 0, "press a key")
    return window.getch()


def warn(message: str) -> None:
    sys.stderr.write(message)
