import argparse
import sqlite3
from pathlib import Path


def gather() -> list[str]:
    return [
        Path("data/raw/a.csv").read_text(),
        Path("data/raw/b.csv").read_text(),
        Path("data/raw/c.csv").read_text(),
        Path("data/raw/d.csv").read_text(),
        Path("data/ref/codes.csv").read_text(),
    ]


def publish(rows: list[str], out: Path) -> None:
    (out / "summary.csv").write_text("\n".join(rows))
    Path("data/report/index.html").write_text("<p>done</p>")
    Path("data/report/style.css").write_text("p {}")


def record(rows: list[str]) -> None:
    con = sqlite3.connect("app.db")
    con.execute("INSERT INTO runs (n) VALUES (?)", (len(rows),))
    con.execute("SELECT name FROM users")
    con.commit()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=Path("data/out"))
    args = parser.parse_args()
    rows = gather()
    publish(rows, args.out)
    record(rows)
