//! Python is read by Python's rules and by what its libraries declare, in any package: what a
//! command of a command-line framework and a parser of the command line take from the human.

use atlas_engine::{analyze, EntryKind, Graph, Options};

/// A project written to a directory of its own, with the libraries it imports as stubs, read, and
/// removed.
fn written(name: &str, files: &[(&str, &str)]) -> Graph {
    let dir = std::env::temp_dir().join(format!("atlas-python-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let manifest = format!("[project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[tool.ty.environment]\nextra-paths = [\"typings\"]\n");
    for (path, text) in std::iter::once(&("pyproject.toml", manifest.as_str())).chain(files) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let g = analyze(&dir, &Options::default());
    let _ = std::fs::remove_dir_all(&dir);
    g.expect("the project parses")
}

const TYPER: &str = r#"from typing import Any, Callable, TypeVar

F = TypeVar("F", bound=Callable[..., Any])

class Typer:
    def __init__(self, **kwargs: Any) -> None: ...
    def command(self, name: str | None = None, **kwargs: Any) -> Callable[[F], F]: ...

def Option(default: Any = ..., *param_decls: str, **kwargs: Any) -> Any: ...
def Argument(default: Any = ..., **kwargs: Any) -> Any: ...
"#;

const CLICK: &str = r#"from typing import Any, Callable, TypeVar

F = TypeVar("F", bound=Callable[..., Any])

class Command:
    def __call__(self, *args: Any, **kwargs: Any) -> Any: ...

def command(*args: Any, **kwargs: Any) -> Callable[[Callable[..., Any]], Command]: ...
def option(*param_decls: str, **kwargs: Any) -> Callable[[F], F]: ...
def argument(*param_decls: str, **kwargs: Any) -> Callable[[F], F]: ...
"#;

/// An entry's domain is what the human hands it on the command line, each as they type it, in the
/// order the code declares it. typer reads a command's parameters: one with a default is an option
/// (`out_dir: Path = ..` is `--out-dir`), one without an argument (`<source>`), and a default or an
/// annotation that calls typer's `Option` or `Argument` says which, `Option` with its flags. click
/// reads the command's decorators: `@click.option("-o", "--out")` is `--out`,
/// `@click.argument("src")` is `<src>`. A script takes what the parser its route parses with
/// declares (`parser.add_argument("title")` is `<title>`), not what another parser does, and the
/// parse is a step that reads the command line, giving the namespace the body keeps.
#[test]
fn a_commands_options_are_the_entrys_domain() {
    let g = written(
        "usage",
        &[
            ("typings/typer/__init__.pyi", TYPER),
            ("typings/click/__init__.pyi", CLICK),
            ("src/usage/__init__.py", ""),
            (
                "src/usage/cli.py",
                r#"from pathlib import Path
from typing import Annotated

import typer

app = typer.Typer()


@app.command()
def run(source: str, out_dir: Path = Path("out"), name: Annotated[str, typer.Argument()] = "x", level: int = typer.Option(1, "--level", "-l"), dry_run: bool = False) -> None:
    print(source, out_dir, name, level, dry_run)
"#,
            ),
            (
                "src/usage/legacy.py",
                r#"import click


@click.command()
@click.option("-o", "--out", default="out")
@click.argument("src")
@click.option("--force", is_flag=True)
def copy(out: str, src: str, force: bool) -> None:
    print(out, src, force)
"#,
            ),
            (
                "scripts/report.py",
                r#"import argparse


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("-o", "--out-file", default="out/report.txt")
    parser.add_argument("title")
    other = argparse.ArgumentParser()
    other.add_argument("--elsewhere")
    args = parser.parse_args()
    print(args.title)


if __name__ == "__main__":
    main()
"#,
            ),
        ],
    );
    let entry = |id: &str| {
        g.node(id).and_then(|n| n.entry.as_ref()).map(|e| (e.kind, e.options.join(" "))).unwrap_or_else(|| panic!("{id} is an entry"))
    };
    assert_eq!(entry("usage.cli.run"), (EntryKind::Cli, "<source> --out-dir <name> --level --dry-run".to_string()));
    assert_eq!(entry("usage.legacy.copy"), (EntryKind::Cli, "--out <src> --force".to_string()));
    assert_eq!(entry("scripts.report"), (EntryKind::Script, "--out-file <title>".to_string()));
    let main = g.node("scripts.report.main").unwrap();
    let parse = main.sites.iter().find(|s| s.command_line.is_some()).expect("main parses its command line");
    assert_eq!(parse.op, "parse_args");
    assert_eq!(parse.gives.iter().map(|h| (h.name.as_str(), h.ty.as_str())).collect::<Vec<_>>(), [("args", "ext:argparse.Namespace")]);
    assert!(g.node("usage.cli.run").unwrap().sites.iter().all(|s| s.command_line.is_none()), "typer parses before the command runs");
}
