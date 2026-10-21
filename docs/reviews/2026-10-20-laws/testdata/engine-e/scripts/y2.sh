# claim 2: renames. One base program, one repository per case.
source ${0:a:h}/base.sh
prog() {  # prog <file> <def-line> <body-lines> <call-in-main>
  {
    print -r -- 'from dataclasses import dataclass'
    print -r -- 'from pathlib import Path'
    print -r -- ''
    print -r -- ''
    print -r -- '@dataclass'
    print -r -- 'class Frame:'
    print -r -- '    values: list[float]'
    print -r -- ''
    print -r -- ''
    print -r -- 'def load(path: Path) -> Frame:'
    print -r -- '    return Frame([float(x) for x in path.read_text().split()])'
    print -r -- ''
    print -r -- ''
    print -r -- "$2"
    print -r -- "$3"
    print -r -- ''
    print -r -- ''
    print -r -- 'def main() -> None:'
    print -r -- '    frame = load(Path("data/in.txt"))'
    print -r -- "    $4"
  } > $1
}
NL=$'\n'
# 2a: plot renamed to draw, nothing else
R=$E/y2a-one-rename; calc_repo $R
prog $R/src/calc/cli.py "def plot(frame: Frame, out: Path) -> None:" "    out.write_text(str(frame.values))" 'plot(frame, Path("out/a.txt"))'
$T/commit.sh $R A >/dev/null
prog $R/src/calc/cli.py "def draw(frame: Frame, out: Path) -> None:" "    out.write_text(str(frame.values))" 'draw(frame, Path("out/a.txt"))'
$T/commit.sh $R "B: plot is draw" >/dev/null
# 2c: plot renamed to draw and gains a primitive parameter (width: int)
R=$E/y2c-rename-and-new-primitive-param; calc_repo $R
prog $R/src/calc/cli.py "def plot(frame: Frame, out: Path) -> None:" "    out.write_text(str(frame.values))" 'plot(frame, Path("out/a.txt"))'
$T/commit.sh $R A >/dev/null
prog $R/src/calc/cli.py "def draw(frame: Frame, out: Path, width: int) -> None:" "    out.write_text(str(frame.values)[:width])" 'draw(frame, Path("out/a.txt"), 80)'
$T/commit.sh $R "B: plot is draw(width)" >/dev/null
# 2c2: plot renamed to draw and its return type changes from None to str (main prints it)
R=$E/y2c2-rename-and-new-return; calc_repo $R
prog $R/src/calc/cli.py "def plot(frame: Frame, out: Path) -> None:" "    out.write_text(str(frame.values))" 'plot(frame, Path("out/a.txt"))'
$T/commit.sh $R A >/dev/null
prog $R/src/calc/cli.py "def draw(frame: Frame, out: Path) -> str:" "    out.write_text(str(frame.values))${NL}    return str(out)" 'print(draw(frame, Path("out/a.txt")))'
$T/commit.sh $R "B: plot is draw -> str" >/dev/null
# 2c3: plot renamed to draw, and it takes a second project type (meta: Meta): typed arrows that differ in a project type
R=$E/y2c3-rename-and-new-project-param; calc_repo $R
prog $R/src/calc/cli.py "def plot(frame: Frame, out: Path) -> None:" "    out.write_text(str(frame.values))" 'plot(frame, Path("out/a.txt"))'
$T/commit.sh $R A >/dev/null
prog $R/src/calc/cli.py "def draw(frame: Frame, out: Path, also: Frame) -> None:" "    out.write_text(str(frame.values + also.values))" 'draw(frame, Path("out/a.txt"), frame)'
$T/commit.sh $R "B: plot is draw(also: Frame)" >/dev/null
# 2d: two unrelated functions at one place in the call graph: mean(xs: list[float]) -> float removed, label(name: str) -> str added
R=$E/y2d-unrelated-pair; calc_repo $R
prog $R/src/calc/cli.py "def mean(xs: list[float]) -> float:" "    return sum(xs) / len(xs)" 'print(mean(frame.values))'
$T/commit.sh $R A >/dev/null
prog $R/src/calc/cli.py "def label(name: str) -> str:" '    return name.upper() + "!"' 'print(label("raw"))'
$T/commit.sh $R "B: mean is gone, label is new" >/dev/null
