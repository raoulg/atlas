from typing import Any

from units.core import Length


def parse(text: Any) -> Any:
    return Length(float(text.removesuffix("m")))
