from shop import model


def ship(order: model.Order) -> model.Label:
    return model.Label(f"{order.reference} ({order.weight} kg)")
