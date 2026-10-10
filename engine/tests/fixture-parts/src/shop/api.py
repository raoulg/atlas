"""Items, served at /api by the app that mounts this one."""

from fastapi import FastAPI

app = FastAPI()


@app.get("/items/{item_id}")
def item(item_id: str) -> dict:
    return {"id": item_id}
