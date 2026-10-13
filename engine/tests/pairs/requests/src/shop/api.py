"""The routes both clients ask for."""

from fastapi import FastAPI

app = FastAPI()


@app.get("/api/items/{item_id}")
def item(item_id: str) -> dict:
    return {"id": item_id}


@app.post("/api/orders")
def order(item_id: str) -> dict:
    return {"ordered": item_id}


@app.get("/api/orders")
def orders() -> list:
    return []
