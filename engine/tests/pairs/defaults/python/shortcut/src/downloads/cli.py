from pathlib import Path

from downloads.bench import made
from downloads.keep import fetch, keep, save_report


def jot(text: str) -> None:
    """A note kept straight in the workbench of `work`."""
    made(Path("work"), "kept.txt").write_text(text)


def main() -> None:
    save_report("draft")
    save_report("final", Path("out/report.txt"))
    fetch("rates", "{}")
    keep("noted", Path("work"))
    jot("noted")
