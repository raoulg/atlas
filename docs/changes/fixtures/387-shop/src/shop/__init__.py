"""A shop that keeps its orders and its bills, each as frames of its own."""

from shop.billing import model as billing
from shop.billing.model import Frame as Bill
from shop.orders import model as orders

__all__ = ["Bill", "billed", "counted", "found", "ordered", "settled", "shown", "sized"]


def ordered(lines: list[str]) -> str:
    kept, copied = orders.Kept(lines), orders.Copied(lines)
    return kept.first() + copied.first()


def billed(cents: list[int]) -> int:
    return billing.Frame(cents).total()


def settled(lines: list[str], cents: list[int]) -> str:
    order, bill = orders.Frame(lines), billing.Frame(cents)
    return f"{order.first()}: {bill.total()}"


def found(frames: list[orders.Frame], at: int) -> str:
    return shown(orders.find(frames, at))


def counted(frames: list[orders.Frame], at: int) -> int:
    return sized(orders.find(frames, at))


def shown(frame: orders.Frame | orders.Missing) -> str:
    match frame:
        case orders.Missing():
            return ""
        case _:
            return frame.first()


def sized(frame: orders.Frame | orders.Missing) -> int:
    match frame:
        case orders.Missing():
            return 0
        case _:
            return len(frame.rows)
