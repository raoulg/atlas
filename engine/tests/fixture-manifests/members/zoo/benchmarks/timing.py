from zoo_keeper.keep import keep


def keep_many(count: int) -> list[str]:
    return [keep("lion") for _ in range(count)]
