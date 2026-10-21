from pathlib import Path

from tool.report import render


def main() -> None:
    Path("report.txt").write_text(render(["done"]))
