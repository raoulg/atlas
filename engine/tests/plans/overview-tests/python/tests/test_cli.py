"""The command's test: it runs the command and keeps a mark that it ran."""

from pathlib import Path

from shelf.cli import main


def test_main() -> None:
    main()
    Path("out/ran.txt").write_text("ok")
