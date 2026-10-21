from units.core import Length


def parse(text: str) -> Length:
    return Length(float(text.removesuffix("m")))
