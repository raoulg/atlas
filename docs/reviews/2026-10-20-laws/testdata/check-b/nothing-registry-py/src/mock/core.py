from collections.abc import Callable

HANDLERS: dict[str, Callable[[], None]] = {}


def register(name: str, fn: Callable[[], None]) -> None:
    HANDLERS[name] = fn


class Registry:
    handlers: dict[str, Callable[[], None]] = {}

    @classmethod
    def register(cls, name: str, fn: Callable[[], None]) -> None:
        cls.handlers[name] = fn


def main() -> int:
    register("a", lambda: None)
    Registry.register("a", lambda: None)
    return len(HANDLERS) + len(Registry.handlers)
