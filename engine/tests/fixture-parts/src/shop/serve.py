"""The app a client talks to: items under /api, orders under /v1/orders."""

from fastapi import APIRouter, FastAPI

from shop.api import app as items

app = FastAPI()
router = APIRouter(prefix="/orders")


@router.post("/")
def create_order(item_id: str) -> dict:
    return {"ordered": item_id}


@app.get("/health")
def health() -> dict:
    return {"ok": True}


app.mount("/api", items)
app.include_router(router, prefix="/v1")
