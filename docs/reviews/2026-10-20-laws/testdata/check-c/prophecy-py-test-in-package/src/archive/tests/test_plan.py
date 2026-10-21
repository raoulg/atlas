from archive.plan import seal


def test_seal_keeps_the_report() -> None:
    assert seal("r") == "r"
