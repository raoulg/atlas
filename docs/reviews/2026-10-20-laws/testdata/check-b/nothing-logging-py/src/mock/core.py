import logging

logger = logging.getLogger(__name__)


def note(msg: str) -> None:
    logger.info(msg)


def show(msg: str) -> None:
    print(msg)


def main() -> int:
    note("a")
    show("b")
    return 0
