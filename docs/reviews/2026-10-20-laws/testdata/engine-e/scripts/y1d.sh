# 1d: a type gains a field: B1 a field of a primitive type (count: int), B2 (from A) a field of a project type (meta: Meta)
source ${0:a:h}/base.sh
R=$E/y1d-type-gains-field; calc_repo $R
cat > $R/src/calc/cli.py <<'P'
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Meta:
    name: str


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def total(frame: Frame) -> float:
    return sum(frame.values)


def describe(meta: Meta) -> str:
    return meta.name


def main() -> None:
    frame = load(Path("data/in.txt"))
    print(total(frame))
    print(describe(Meta("raw")))
P
$T/commit.sh $R "A"
git -C $R branch base
sed -i '' 's/^    values: list\[float\]$/    values: list[float]\n    count: int = 0/' $R/src/calc/cli.py
$T/commit.sh $R "B1: Frame gains count: int"
git -C $R checkout -q -b meta base
sed -i '' 's/^    values: list\[float\]$/    values: list[float]\n    meta: Meta | None = None/' $R/src/calc/cli.py
$T/commit.sh $R "B2: Frame gains meta: Meta"
git -C $R checkout -q main
