def from_env() -> str:
    return "env"


def from_file(p: str) -> str:
    return p


def greet(name: str) -> int:
    return len(name)


def plain_type(p: str, e: bool) -> int:
    name = from_env() if e else from_file(p)
    return greet(name)


def main(p: str) -> int:
    return plain_type(p, True)
