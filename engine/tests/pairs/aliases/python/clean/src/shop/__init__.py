"""A shop that ships orders: the order it is handed, the label it gives back."""

from shop.model import Label, Order
from shop.post import ship

__all__ = ["Label", "Order", "ship"]
