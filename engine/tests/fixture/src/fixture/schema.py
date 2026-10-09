"""The central type."""

from pydantic import BaseModel


class Node(BaseModel):
    id: str


class Graph(BaseModel):
    """Nodes and edges."""

    nodes: list[Node] = []

    def add(self, node: Node) -> None:
        self.nodes.append(node)

    def first(self):
        return self.nodes[0]
