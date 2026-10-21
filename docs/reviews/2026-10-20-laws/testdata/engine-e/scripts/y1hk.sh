source ${0:a:h}/base.sh
# 1h: out/x.txt is written by calc.a.save in A and by calc.b.save in B (two functions with one short name)
R=$E/y1h-writer-same-short-name; calc_repo $R
gen() {
printf 'from pathlib import Path\n\n\ndef save() -> None:\n    Path("%s").write_text("a")\n' "$1" > $R/src/calc/a.py
printf 'from pathlib import Path\n\n\ndef save() -> None:\n    Path("%s").write_text("b")\n' "$2" > $R/src/calc/b.py
cat > $R/src/calc/cli.py <<'P'
from calc import a, b


def main() -> None:
    a.save()
    b.save()
P
}
gen out/x.txt out/y.txt; $T/commit.sh $R "A: a.save writes x" >/dev/null
gen out/y.txt out/x.txt; $T/commit.sh $R "B: b.save writes x" >/dev/null
# 1k: a field of Frame is renamed: meta -> info (same type Meta)
R=$E/y1k-field-renamed; calc_repo $R
gen2() {
cat > $R/src/calc/cli.py <<P
from dataclasses import dataclass


@dataclass
class Meta:
    name: str


@dataclass
class Frame:
    values: list[float]
    $1: Meta


def build() -> Frame:
    return Frame([1.0], Meta("raw"))


def main() -> None:
    print(build().values)
P
}
gen2 meta; $T/commit.sh $R "A: meta" >/dev/null
gen2 info; $T/commit.sh $R "B: info" >/dev/null
