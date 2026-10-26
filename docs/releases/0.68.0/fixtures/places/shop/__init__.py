"""Cancel an order on the shop's server."""

import requests


def cancel(order: int) -> None:
    requests.delete(f"https://shop.example/orders/{order}")


def main() -> None:
    cancel(42)
