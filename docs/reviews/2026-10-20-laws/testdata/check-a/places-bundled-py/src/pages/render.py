from pathlib import Path


def render(title: str) -> str:
    template = (Path(__file__).parent / "template.html").read_text()
    return template.replace("{title}", title)
