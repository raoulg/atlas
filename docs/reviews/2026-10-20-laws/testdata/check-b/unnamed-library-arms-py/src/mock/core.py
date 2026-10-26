import json
import tomllib


def count(d: dict[str, object]) -> int:
    return len(d)


def library_arms(s: str, j: bool) -> int:
    data = json.loads(s) if j else tomllib.loads(s)
    return count(data)


def main(s: str) -> int:
    return library_arms(s, True)
