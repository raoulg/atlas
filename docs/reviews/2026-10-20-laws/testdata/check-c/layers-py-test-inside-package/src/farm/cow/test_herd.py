from farm.barn.store import keep
from farm.cow.herd import count


def test_count() -> None:
    assert count(2) == 4
    assert keep(1) == 3
