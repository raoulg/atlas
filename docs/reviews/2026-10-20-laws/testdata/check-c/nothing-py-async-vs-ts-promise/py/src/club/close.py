class Kernel:
    async def stop(self) -> None: ...


async def shut(k: Kernel) -> None:
    await k.stop()


async def close_all(ks: list[Kernel]) -> int:
    for k in ks:
        await shut(k)
    return len(ks)
