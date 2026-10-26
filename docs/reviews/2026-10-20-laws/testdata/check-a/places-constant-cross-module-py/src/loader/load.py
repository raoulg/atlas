from loader.config import DATA


def load() -> str:
    return (DATA / "raw.csv").read_text()
