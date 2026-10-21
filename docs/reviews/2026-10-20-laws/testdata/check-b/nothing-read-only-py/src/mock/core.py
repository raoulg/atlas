from pathlib import Path


def peek(p: Path) -> None:
    p.read_text()


def main() -> int:
    peek(Path("x.txt"))
    return 0
