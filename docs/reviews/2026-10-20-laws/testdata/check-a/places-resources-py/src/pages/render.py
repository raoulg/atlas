from importlib.resources import files


def render(title: str) -> str:
    template = files("pages").joinpath("template.html").read_text()
    return template.replace("{title}", title)
