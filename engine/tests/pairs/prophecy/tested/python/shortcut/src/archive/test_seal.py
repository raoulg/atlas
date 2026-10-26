from archive.plan import seal


def sealed_twice(report: str) -> str:
    return seal(seal(report))


def test_sealing_twice_is_sealing_once() -> None:
    assert sealed_twice("r") == seal("r")
