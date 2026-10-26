import requests


def item(item_id: str) -> dict:
    return requests.get(f"http://localhost:8000/api/v2/items/{item_id}").json()
