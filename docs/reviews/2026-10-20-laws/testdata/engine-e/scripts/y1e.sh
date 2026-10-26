# 1e: `main` hands `save` another path; `save` writes what it is handed. The place is made whole by the caller
source ${0:a:h}/base.sh
R=$E/y1e-caller-makes-path; calc_repo $R
cat > $R/src/calc/cli.py <<'P'
from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def save(xs: list[float], out: Path) -> None:
    out.write_text(" ".join(str(x) for x in xs))


def main() -> None:
    xs = load(Path("data/in.txt"))
    save(xs, Path("out/a.txt"))
P
$T/commit.sh $R "A"
sed -i '' 's#out/a.txt#out/b.txt#' $R/src/calc/cli.py
$T/commit.sh $R "B: main hands out/b.txt"
