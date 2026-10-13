"""The same client, with each path behind a key looked up at run time and an environment variable
that may stand in for it."""

import os

import requests

PATHS = {"item": "/api/items", "order": "/api/orders"}


def endpoint(key: str) -> str:
    return os.environ["API_BASE_URL"] + os.environ.get(f"{key.upper()}_PATH", PATHS[key])


def item(item_id):
    return requests.get(f"{endpoint('item')}/{item_id}").json()


def order(item_id):
    requests.post(endpoint("order"), data=item_id)
