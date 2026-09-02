import json
from pathlib import Path

import duckdb

from fixture.schema import Graph

from .base import Mixin, Sink


class DuckSink(Sink, Mixin):
    def __init__(self, path: Path):
        self.con = duckdb.connect(str(path))

    def write(self, graph: Graph) -> None:
        self.con.execute("INSERT INTO nodes VALUES (?)", [len(graph.nodes)])
        Path("out.json").write_text(json.dumps({"n": len(graph.nodes)}))
        self.close()
