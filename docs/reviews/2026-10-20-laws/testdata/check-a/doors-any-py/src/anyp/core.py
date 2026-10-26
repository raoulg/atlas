import json
from typing import Any


def parse(text: Any) -> Any:
    return json.loads(text)
