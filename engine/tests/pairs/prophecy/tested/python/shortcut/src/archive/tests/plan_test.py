from archive.plan import seal


def test_a_sealed_report_keeps_its_text() -> None:
    assert seal(" r ") == "r"
