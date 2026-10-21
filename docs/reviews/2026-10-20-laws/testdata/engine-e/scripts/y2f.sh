# 2f: plot renamed to draw, nothing else; plot writes the path it is handed, and no caller in the repository completes it
source ${0:a:h}/base.sh
R=$E/y2f-rename-own-named-place; calc_repo $R
gen() {
cat > $R/src/calc/cli.py <<P
from pathlib import Path


def $1(values: list[float], out: Path) -> None:
    out.write_text(str(values))


def main() -> None:
    print(sum([1.0, 2.0]))
P
}
gen plot; $T/commit.sh $R "A: plot" >/dev/null
gen draw; $T/commit.sh $R "B: plot is draw" >/dev/null
