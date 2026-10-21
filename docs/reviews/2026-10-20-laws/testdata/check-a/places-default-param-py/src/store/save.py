from pathlib import Path


def save(rows: list[str], path: Path = Path("out.csv")) -> None:
    path.write_text("\n".join(rows))
