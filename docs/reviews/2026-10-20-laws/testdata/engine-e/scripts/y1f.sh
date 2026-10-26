# 1f: upstream, main replaces load(path) by fetch(path) (same arrow Path -> Frame); smooth and plot after it are unchanged
source ${0:a:h}/base.sh
R=$E/y1f-upstream-producer-swap; calc_repo $R
gen() {
cat > $R/src/calc/cli.py <<P
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def fetch(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split(",")])


def smooth(frame: Frame) -> Frame:
    return Frame(frame.values[1:])


def plot(frame: Frame) -> None:
    Path("out/plot.txt").write_text(str(frame.values))


def main() -> None:
    raw = $1(Path("data/in.txt"))
    frame = smooth(raw)
    plot(frame)
P
}
gen load; $T/commit.sh $R "A: load" >/dev/null
gen fetch; $T/commit.sh $R "B: fetch" >/dev/null
