# 1b: the callee `save` changes the file it writes (its own literal); `main`, which calls it, does not change
source ${0:a:h}/base.sh
R=$E/y1b-callee-effect; calc_repo $R
cat > $R/src/calc/cli.py <<'P'
from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def save(xs: list[float]) -> None:
    Path("out/a.txt").write_text(" ".join(str(x) for x in xs))


def main() -> None:
    xs = load(Path("data/in.txt"))
    save(xs)
P
$T/commit.sh $R "A"
sed -i '' 's#out/a.txt#out/b.txt#' $R/src/calc/cli.py
$T/commit.sh $R "B: save writes out/b.txt"
