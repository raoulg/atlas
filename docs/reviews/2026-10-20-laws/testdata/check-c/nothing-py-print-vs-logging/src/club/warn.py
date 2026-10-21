import logging
import sys

log = logging.getLogger(__name__)


def say_print(n: int) -> None:
    print(f"{n} is large", file=sys.stderr)


def say_log(n: int) -> None:
    log.warning("%d is large", n)


def check(n: int) -> int:
    if n > 10:
        say_print(n)
        say_log(n)
    return n
