"""Litestar's handlers: decorators named for the method they serve, and one handed its methods."""

from litestar import HttpMethod, get, route


@get("/stock/{item_id:str}")
def stock(item_id: str) -> dict:
    return {"item": item_id}


@get()
def overview() -> dict:
    return {"items": 0}


@route("/restock", http_method=[HttpMethod.POST, "PUT"])
def restock() -> str:
    return "restocked"
