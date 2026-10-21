import os


def env_item() -> None:
    os.environ["DB"] = "x.db"


def env_update() -> None:
    os.environ.update({"DB": "x.db"})


def env_default() -> None:
    os.environ.setdefault("DB", "x.db")


def env_put() -> None:
    os.putenv("DB", "x.db")


def main() -> str:
    env_item()
    env_update()
    env_default()
    env_put()
    return os.environ["DB"]
