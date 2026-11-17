from shop.model import Label, Order


def ship(order: Order) -> Label:
    return Label(f"{order.reference} ({order.weight} kg)")
