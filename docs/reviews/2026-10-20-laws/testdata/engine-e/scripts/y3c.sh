# claim 3: a bundle across a closed step's border. report(frame) takes the frame to head and tail in A, and also to middle in B;
# report's own arrow (its signature, the file it writes) is the same in both
source ${0:a:h}/base.sh
R=$E/y3c-bundle-count; calc_repo $R
gen() {
cat > $R/src/calc/cli.py <<P
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def head(frame: Frame) -> Frame:
    return Frame(frame.values[:1])


def middle(frame: Frame) -> Frame:
    return Frame(frame.values[1:-1])


def tail(frame: Frame) -> Frame:
    return Frame(frame.values[-1:])


def report(frame: Frame) -> None:
    first = head(frame)
    mid = $1
    last = tail(frame)
    Path("out/report.txt").write_text(str(first.values + mid.values + last.values))


def main() -> None:
    frame = load(Path("data/in.txt"))
    report(frame)
P
}
gen "first"; $T/commit.sh $R "A: head, tail" >/dev/null
gen "middle(frame)"; $T/commit.sh $R "B: head, middle, tail" >/dev/null
