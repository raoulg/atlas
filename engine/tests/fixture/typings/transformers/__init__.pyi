from typing import Any

from torch import Tensor

class PreTrainedModel:
    def generate(self, inputs: Tensor | None = None, **kwargs: Any) -> Tensor: ...
