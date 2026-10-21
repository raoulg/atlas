from fastapi import FastAPI

from shop import prophecy

app = FastAPI()


@prophecy()
@app.get("/api/items/{item_id}")
def item(item_id: str) -> dict:
    raise NotImplementedError
