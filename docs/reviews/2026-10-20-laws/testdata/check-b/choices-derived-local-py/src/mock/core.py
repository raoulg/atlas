from dataclasses import dataclass


@dataclass
class Args:
    mode: str


def direct(args: Args) -> int:
    a = 1 if args.mode == "fast" else 2
    b = 3 if args.mode == "fast" else 4
    return a + b


def derived(args: Args) -> int:
    m = args.mode
    a = 1 if m == "fast" else 2
    b = 3 if m == "fast" else 4
    return a + b


def normalised(mode: str) -> int:
    a = 1 if mode.lower() == "fast" else 2
    b = 3 if mode.lower() == "fast" else 4
    return a + b


def main(mode: str) -> int:
    return direct(Args(mode)) + derived(Args(mode)) + normalised(mode)
