"""A store of notes and the line a report makes of it: the route makes the line itself, and throws away what the step
that saves it gives back."""

from dataclasses import dataclass


@dataclass
class Store:
    path: str
    count: int


def save(store: Store, line: str) -> str:
    with open(store.path, "a") as f:
        f.write(line + "\n")
    return line


def run() -> None:
    store = Store("notes.txt", 3)
    line = f"{store.path}: {store.count} notes"
    save(store, line)
    print(line)
