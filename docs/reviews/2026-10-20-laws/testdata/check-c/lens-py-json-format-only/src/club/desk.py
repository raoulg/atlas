import json
from dataclasses import asdict, dataclass
from typing import IO


@dataclass
class Visit:
    member: str
    day: str


class Desk:
    """Writes a visit to the stream it is handed; reads member names from another stream."""

    def log(self, visit: Visit, out: IO[str]) -> None:
        json.dump(asdict(visit), out)

    def members(self, src: IO[str]) -> list[str]:
        return json.load(src)
