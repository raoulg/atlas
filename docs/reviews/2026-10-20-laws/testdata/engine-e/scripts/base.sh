# base.sh: shared pieces for the calc mocks. Source it; then call `calc_repo <dir>` to lay out the package.
# E: where the repositories are built (default: atlas-laws-yoneda in the temporary directory); T: the tools beside these scripts.
E=${E:-${TMPDIR:-/tmp}/atlas-laws-yoneda}
T=${0:a:h:h}/tools
calc_repo() {
  $T/mkrepo.sh "$1"
  mkdir -p "$1/src/calc"
  cat > "$1/pyproject.toml" <<'P'
[project]
name = "calc"
version = "0.1.0"
requires-python = ">=3.12"

[project.scripts]
calc = "calc.cli:main"
P
  cat > "$1/src/calc/__init__.py" <<'P'
from calc.cli import main

__all__ = ["main"]
P
}
