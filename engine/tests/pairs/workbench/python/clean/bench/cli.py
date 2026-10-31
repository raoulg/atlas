from pathlib import Path

from bench.keep import clear, save_log, save_report


def main() -> None:
    save_report(Path("."), "all well")
    save_log(Path("."), "ran")
    clear(Path("."))
