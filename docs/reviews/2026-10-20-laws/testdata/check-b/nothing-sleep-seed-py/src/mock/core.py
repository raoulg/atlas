import random
import time


def wait(seconds: float) -> None:
    time.sleep(seconds)


def seed() -> None:
    random.seed(0)


def main() -> int:
    wait(0.1)
    seed()
    return random.randint(0, 9)
