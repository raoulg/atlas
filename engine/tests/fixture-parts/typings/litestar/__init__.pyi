from enum import Enum
from typing import Any, Callable, Sequence

class HttpMethod(str, Enum):
    GET = "GET"
    POST = "POST"
    PUT = "PUT"

class HTTPRouteHandler:
    def __call__(self, fn: Callable[..., Any]) -> Any: ...

class get(HTTPRouteHandler):
    def __init__(self, path: str | Sequence[str] | None = None, **kwargs: Any) -> None: ...

class post(HTTPRouteHandler):
    def __init__(self, path: str | Sequence[str] | None = None, **kwargs: Any) -> None: ...

class route(HTTPRouteHandler):
    def __init__(self, path: str | Sequence[str] | None = None, *, http_method: str | Sequence[str], **kwargs: Any) -> None: ...
