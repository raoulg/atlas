import json
from pathlib import Path


def save(data: dict[str, int], path: Path) -> None:
    path.write_text(json.dumps(data))


def load(path: Path) -> dict[str, int]:
    return json.loads(path.read_text())


def refresh(path: Path) -> dict[str, int]:
    save({"a": 1}, path)
    return load(path)


def main() -> None:
    print(refresh(Path("cache.json")))
