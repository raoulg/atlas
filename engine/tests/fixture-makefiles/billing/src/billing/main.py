from pathlib import Path


def main() -> None:
    Path("out/billing.txt").write_text("billing")
