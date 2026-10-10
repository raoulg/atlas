from typing import Any

class Messages:
    def create(self, *, model: str, max_tokens: int, messages: list[Any]) -> Any: ...

class Anthropic:
    messages: Messages
    def __init__(self, api_key: str | None = None) -> None: ...
