source ${0:a:h}/base.sh
# 2b: two functions with the same relations, both renamed at once
R=$E/y2b-two-renames; calc_repo $R
two() {
cat > $R/src/calc/cli.py <<P
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def $1(frame: Frame) -> Frame:
    return frame


def $2(frame: Frame) -> Frame:
    return frame


def main() -> None:
    frame = load(Path("data/in.txt"))
    $1(frame)
    $2(frame)
P
}
two first second; $T/commit.sh $R A >/dev/null
two alpha beta; $T/commit.sh $R "B: both renamed" >/dev/null
# 2e: a class renamed, its methods move with it
R=$E/y2e-class-rename; calc_repo $R
cls() {
cat > $R/src/calc/cli.py <<P
from pathlib import Path


class $1:
    def __init__(self, values: list[float]) -> None:
        self.values = values

    def mean(self) -> float:
        return sum(self.values) / len(self.values)

    def trimmed(self) -> "$1":
        return $1(self.values[1:])


def load(path: Path) -> $1:
    return $1([float(x) for x in path.read_text().split()])


def smooth(frame: $1) -> $1:
    return frame.trimmed()


def main() -> None:
    frame = smooth(load(Path("data/in.txt")))
    print(frame.mean())
P
}
cls Frame; $T/commit.sh $R A >/dev/null
cls Series; $T/commit.sh $R "B: Frame is Series" >/dev/null
