from archive import prophecy


@prophecy()
class Vault:
    def keep(self, report: str) -> None: ...


@prophecy()
def seal(report: str) -> str:
    raise NotImplementedError
