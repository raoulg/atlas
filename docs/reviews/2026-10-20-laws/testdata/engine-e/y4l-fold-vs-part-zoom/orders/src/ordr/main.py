from pathlib import Path


def main() -> None:
    text = Path("data/orders.csv").read_text()
    Path("out/a/1.txt").write_text(text)
    Path("out/a/2.txt").write_text(text)
    Path("out/a/3.txt").write_text(text)
