from pathlib import Path


def write_header(path: Path) -> None:
    path.write_text("name\n")


def export(rows: list[str], path: Path) -> None:
    write_header(path)
    with path.open("a") as f:
        f.write("\n".join(rows))


def main() -> None:
    export(["a", "b"], Path("out.csv"))
