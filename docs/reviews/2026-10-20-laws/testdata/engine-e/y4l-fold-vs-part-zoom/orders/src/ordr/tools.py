from pathlib import Path


def rebuild(text: str) -> None:
    Path("out/a/4.txt").write_text(text)
    Path("out/a/5.txt").write_text(text)
    Path("out/a/6.txt").write_text(text)
    Path("out/a/7.txt").write_text(text)
    Path("out/b/1.txt").write_text(text)
