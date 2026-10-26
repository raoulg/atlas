"""pipeline = summarize . parse . load: Raw and Parsed lie between the steps."""
from dataclasses import dataclass
from pathlib import Path

SRC = Path("data/in.txt")
STOP = Path("data/stop.txt")
OUT = Path("data/report.txt")
@dataclass
class Raw: text: str
@dataclass
class Parsed: words: list[str]
@dataclass
class Report: count: int
def load() -> Raw:
    return Raw(SRC.read_text())

def parse(raw: Raw) -> Parsed:
    stop = set(STOP.read_text().split())
    return Parsed([w for w in raw.text.split() if w not in stop])

def summarize(p: Parsed) -> Report:
    r = Report(len(p.words))
    OUT.write_text(str(r.count))
    return r

def pipeline() -> Report:
    return summarize(parse(load()))

def run() -> None:
    print(pipeline().count)
