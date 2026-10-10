from typing import Any, dataclass_transform

@dataclass_transform(kw_only_default=True)
class BaseModel:
    def __init__(self, **data: Any) -> None: ...
