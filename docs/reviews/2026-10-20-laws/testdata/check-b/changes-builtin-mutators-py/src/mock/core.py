from dataclasses import dataclass, field


@dataclass
class Box:
    xs: list[int] = field(default_factory=list)
    d: dict[str, int] = field(default_factory=dict)
    s: set[int] = field(default_factory=set)


def l_reverse(b: Box) -> None: b.xs.reverse()
def l_clear(b: Box) -> None: b.xs.clear()
def l_remove(b: Box) -> None: b.xs.remove(1)
def l_sort(b: Box) -> None: b.xs.sort()
def d_update(b: Box) -> None: b.d.update({"a": 1})
def d_clear(b: Box) -> None: b.d.clear()
def d_pop(b: Box) -> None: b.d.pop("a")
def d_popitem(b: Box) -> None: b.d.popitem()
def d_setdefault(b: Box) -> None: b.d.setdefault("a", 1)
def s_add(b: Box) -> None: b.s.add(1)
def s_clear(b: Box) -> None: b.s.clear()
def s_discard(b: Box) -> None: b.s.discard(1)


def main() -> int:
    for f in [l_reverse, l_clear, l_remove, l_sort, d_update, d_clear, d_pop, d_popitem, d_setdefault, s_add, s_clear, s_discard]:
        f(Box())
    return 0
