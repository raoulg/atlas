# 1g: load is byte-identical in A and B; only its caller changes: main calls it in A, main no longer calls it in B
source ${0:a:h}/base.sh
R=$E/y1g-caller-drops-call; calc_repo $R
gen() {
cat > $R/src/calc/cli.py <<P
from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def main() -> None:
    xs = $1
    print(sum(xs))
P
}
gen 'load(Path("data/in.txt"))'; $T/commit.sh $R "A: main calls load" >/dev/null
gen '[1.0, 2.0]'; $T/commit.sh $R "B: main does not call load" >/dev/null
