# 1c: the file figures/spots.txt is written by plot in A and by Trend.save in B
source ${0:a:h}/base.sh
R=$E/y1c-writer-moves; calc_repo $R
HEAD_='from pathlib import Path


class Frame:
    def __init__(self, values: list[float]) -> None:
        self.values = values


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def smooth(frame: Frame) -> Frame:
    return Frame(frame.values[1:])
'
print -r -- "$HEAD_" > $R/src/calc/cli.py
cat >> $R/src/calc/cli.py <<'P'


def plot(frame: Frame, out: Path) -> None:
    out.write_text(" ".join(str(v) for v in frame.values))


def main() -> None:
    frame = smooth(load(Path("data/raw.csv")))
    plot(frame, Path("figures/spots.txt"))
P
$T/commit.sh $R "A: plot writes"
print -r -- "$HEAD_" > $R/src/calc/cli.py
cat >> $R/src/calc/cli.py <<'P'


class Trend:
    def __init__(self, frame: Frame) -> None:
        self.frame = frame

    def save(self, out: Path) -> None:
        out.write_text(" ".join(str(v) for v in self.frame.values))


def main() -> None:
    frame = smooth(load(Path("data/raw.csv")))
    Trend(frame).save(Path("figures/spots.txt"))
P
$T/commit.sh $R "B: Trend.save writes"
