"""Build an encoder from codes, then save it: `save` runs on the encoder and reaches it only through `self`, its fields
read by its own code and by a call on one of them."""

import json
from pathlib import Path


class Encoder:
    def __init__(self, codes: list[int]) -> None:
        self.codes: list[int] = sorted(set(codes))
        self.index: dict[int, int] = {c: i for i, c in enumerate(self.codes)}

    def save(self, path: Path) -> None:
        pairs = list(self.index.items())
        data = {"codes": self.codes, "pairs": pairs}
        path.write_text(json.dumps(data))


def run(codes: list[int], path: Path) -> None:
    encoder = Encoder(codes)
    encoder.save(path)
