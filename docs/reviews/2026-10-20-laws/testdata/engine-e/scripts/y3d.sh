# claim 3: a folded directory (a place that stands for others). export writes out/1..7.txt in A, and out/8.txt too in B
source ${0:a:h}/base.sh
R=$E/y3d-folded-dir-count; calc_repo $R
gen() {
{ print -r -- 'from pathlib import Path'; print -r -- ''; print -r -- ''
  print -r -- 'def export(text: str) -> None:'
  for i in $(seq 1 $1); do print -r -- "    Path(\"out/$i.txt\").write_text(text)"; done
  print -r -- ''; print -r -- ''
  print -r -- 'def main() -> None:'
  print -r -- '    export(Path("data/in.txt").read_text())'
} > $R/src/calc/cli.py
}
gen 7; $T/commit.sh $R "A: seven files" >/dev/null
gen 8; $T/commit.sh $R "B: eight files" >/dev/null
