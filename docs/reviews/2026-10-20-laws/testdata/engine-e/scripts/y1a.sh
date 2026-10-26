# 1a: a function's body changes (arithmetic, and in a second commit one more library call), its arrow does not
source ${0:a:h}/base.sh
R=$E/y1a-body-only; calc_repo $R
cat > $R/src/calc/cli.py <<'P'
from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def total(xs: list[float]) -> float:
    return sum(xs)


def main() -> None:
    xs = load(Path("data/in.txt"))
    print(total(xs))
P
$T/commit.sh $R "A"
sed -i '' 's/    return sum(xs)$/    return sum(xs) * 2.0 - 1.0/' $R/src/calc/cli.py
$T/commit.sh $R "B1: arithmetic in total"
sed -i '' 's/    return sum(xs) \* 2.0 - 1.0$/    return round(sorted(xs)[0] + sum(xs), 2)/' $R/src/calc/cli.py
$T/commit.sh $R "B2: total calls sorted and round"
git -C $R log --oneline
