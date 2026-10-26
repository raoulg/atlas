"""How a report is kept: trimmed now, sealed once sealing is built."""

from archive import prophecy


def run(report: str) -> str:
    return report.strip()


@prophecy(supersedes="archive.plan.run")
def seal(report: str) -> str:
    raise NotImplementedError
