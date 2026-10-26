async def settle() -> None:
    pass


def settle_sync() -> None:
    pass


async def main() -> int:
    await settle()
    settle_sync()
    return 0
