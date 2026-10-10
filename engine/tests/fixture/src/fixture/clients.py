"""Clients: building one moves nothing; a request and a model's answer do."""

from typing import Any

import requests
from anthropic import Anthropic


def content_type(url: str) -> Any:
    response = requests.get(url)
    return response.headers.get("content-type")


def ask(question: str) -> str:
    client = Anthropic()
    reply = client.messages.create(model="m", max_tokens=10, messages=[question])
    return str(reply)
