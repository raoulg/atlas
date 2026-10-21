from pathlib import Path


def write_report(text: str) -> None:
    Path("report.txt").write_text(text)


def main() -> None:
    write_report("done")
