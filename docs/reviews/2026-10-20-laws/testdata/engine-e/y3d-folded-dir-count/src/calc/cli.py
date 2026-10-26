from pathlib import Path


def export(text: str) -> None:
    Path("out/1.txt").write_text(text)
    Path("out/2.txt").write_text(text)
    Path("out/3.txt").write_text(text)
    Path("out/4.txt").write_text(text)
    Path("out/5.txt").write_text(text)
    Path("out/6.txt").write_text(text)
    Path("out/7.txt").write_text(text)
    Path("out/8.txt").write_text(text)


def main() -> None:
    export(Path("data/in.txt").read_text())
