from shop.model import Label
from shop.model import Order as Parcel


def ship(order: Parcel) -> Label:
    return Label(f"{order.reference} ({order.weight} kg)")
