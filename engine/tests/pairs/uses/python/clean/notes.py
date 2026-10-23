"""A store of notes and the line a report makes of it: each value goes to the step that uses it, and what a step
gives back the route uses."""

from dataclasses import dataclass


@dataclass
class Store:
    path: str
    count: int


def describe(store: Store) -> str:
    return f"{store.path}: {store.count} notes"


def save(store: Store, line: str) -> str:
    with open(store.path, "a") as f:
        f.write(line + "\n")
    return line


def run() -> None:
    store = Store("notes.txt", 3)
    line = describe(store)
    kept = save(store, line)
    print(kept)
