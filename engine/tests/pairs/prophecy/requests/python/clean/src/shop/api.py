"""The shop's items over HTTP, and the version of the route that will replace it."""

from fastapi import FastAPI

from shop import prophecy

app = FastAPI()


@app.get("/api/items/{item_id}")
def item(item_id: str) -> dict:
    return {"id": item_id}


@prophecy(supersedes="shop.api.item")
@app.get("/api/v2/items/{item_id}")
def item_v2(item_id: str) -> dict:
    raise NotImplementedError
