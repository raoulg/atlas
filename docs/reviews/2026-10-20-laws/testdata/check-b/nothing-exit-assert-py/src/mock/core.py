import sys


def die(msg: str) -> None:
    sys.exit(msg)


def die_raise(msg: str) -> None:
    raise SystemExit(msg)


def validate(x: int) -> None:
    assert x > 0


def main() -> int:
    validate(1)
    die_raise("c")
    die("c")
    return 0
