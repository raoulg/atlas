"""Tiles kept in a file: looked up by a point, fetched for the area around it when none holds it."""

import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class Point:
    x: float
    y: float


@dataclass(frozen=True)
class Area:
    left: float
    bottom: float
    right: float
    top: float

    def holds(self, at: Point) -> bool:
        return self.left <= at.x <= self.right and self.bottom <= at.y <= self.top

    @classmethod
    def around(cls, at: Point, margin: float) -> "Area":
        return cls(at.x - margin, at.y - margin, at.x + margin, at.y + margin)


@dataclass(frozen=True)
class Tile:
    area: Area
    name: str


class Tiles:
    """Puts a tile for an area and gets it back by a point it holds: the point is a key."""

    def __init__(self, path: Path):
        self.path = path

    def tile(self, at: Point) -> Tile:
        return self.find(at) or self.fetch(Area.around(at, 1.0))

    def find(self, at: Point) -> Tile | None:
        kept = json.loads(self.path.read_text())
        tile = Tile(Area(**kept["area"]), kept["name"])
        return tile if tile.area.holds(at) else None

    def fetch(self, area: Area) -> Tile:
        tile = Tile(area, f"{area.left}_{area.bottom}")
        self.path.write_text(json.dumps(asdict(tile)))
        return tile
