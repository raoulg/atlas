# claim 3: packages as closed boxes. A shop: cli -> pricing -> store, sharing the type Order.
# B1 (branch inside): a helper inside store changes its own arrow; nothing at store's border changes.
# B2 (branch border): store's save_orders writes out/orders.json instead of out/orders.csv: a line at store's border changes.
source ${0:a:h}/base.sh
R=$E/y3a-package-border; $T/mkrepo.sh $R
mkdir -p $R/src/shop/store $R/src/shop/pricing
cat > $R/pyproject.toml <<'P'
[project]
name = "shop"
version = "0.1.0"
requires-python = ">=3.12"

[project.scripts]
shop = "shop.cli:main"
P
cat > $R/src/shop/__init__.py <<'P'
from shop.cli import main

__all__ = ["main"]
P
cat > $R/src/shop/model.py <<'P'
from dataclasses import dataclass


@dataclass
class Order:
    item: str
    amount: float
P
cat > $R/src/shop/pricing/__init__.py <<'P'
from shop.pricing.rules import priced

__all__ = ["priced"]
P
cat > $R/src/shop/pricing/rules.py <<'P'
from shop.model import Order


def priced(orders: list[Order]) -> list[Order]:
    return [Order(o.item, o.amount * 1.21) for o in orders]
P
cat > $R/src/shop/store/__init__.py <<'P'
from shop.store.files import read_orders, save_orders

__all__ = ["read_orders", "save_orders"]
P
cat > $R/src/shop/store/files.py <<'P'
from pathlib import Path

from shop.model import Order


def line(order: Order) -> str:
    return f"{order.item},{order.amount}"


def read_orders() -> list[Order]:
    rows = Path("data/orders.csv").read_text().splitlines()
    return [Order(r.split(",")[0], float(r.split(",")[1])) for r in rows]


def save_orders(orders: list[Order]) -> None:
    Path("out/orders.csv").write_text("\n".join(line(o) for o in orders))
P
cat > $R/src/shop/cli.py <<'P'
from shop.pricing import priced
from shop.store import read_orders, save_orders


def main() -> None:
    save_orders(priced(read_orders()))
P
$T/commit.sh $R "A" >/dev/null
git -C $R branch base
git -C $R checkout -q -b inside
sed -i '' 's/^def line(order: Order) -> str:$/def line(order: Order, sep: str) -> str:/; s/return f"{order.item},{order.amount}"/return f"{order.item}{sep}{order.amount}"/; s/line(o) for o in orders/line(o, ",") for o in orders/' $R/src/shop/store/files.py
$T/commit.sh $R "B1: line takes sep" >/dev/null
git -C $R checkout -q -b border base
sed -i '' 's#out/orders.csv#out/orders.json#' $R/src/shop/store/files.py
$T/commit.sh $R "B2: save_orders writes json" >/dev/null
git -C $R checkout -q main
git -C $R log --all --oneline --graph
