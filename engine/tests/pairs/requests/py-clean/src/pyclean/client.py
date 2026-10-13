"""The backend's routes, asked for by the path they are served at; only the host comes from the
environment."""

import os

import requests

BASE = os.environ.get("API_BASE_URL", "http://localhost:8000")


def item(item_id: str) -> dict:
    return requests.get(f"{BASE}/api/items/{item_id}").json()


def order(item_id: str) -> None:
    requests.post(f"{BASE}/api/orders", data=item_id)
