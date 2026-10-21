from archive.plan import seal


def test_seal_is_idempotent() -> None:
    assert seal(seal("r")) == seal("r")
