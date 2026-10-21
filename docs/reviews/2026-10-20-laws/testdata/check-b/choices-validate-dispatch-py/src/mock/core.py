MODES = ("fast", "slow")


def run(mode: str) -> int:
    if mode not in ("fast", "slow"):
        raise ValueError(mode)
    return 1 if mode == "fast" else 2


def run_by_table(mode: str) -> int:
    if mode not in MODES:
        raise ValueError(mode)
    return 1 if mode == "fast" else 2


def main(mode: str) -> int:
    return run(mode) + run_by_table(mode)
