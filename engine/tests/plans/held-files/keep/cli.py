"""The command: cache a text under `tmp/cache`, and write the day's reports itself."""

from pathlib import Path

from keep.cache import Cache


def main() -> None:
    Cache(Path("tmp/cache")).save("today", "hello")
    for name in ["mon", "tue", "wed", "thu", "fri"]:
        Path(f"reports/{name}.txt").write_text(name)
    Path("reports/week.txt").write_text("week")
    Path("summary.txt").write_text("done")
