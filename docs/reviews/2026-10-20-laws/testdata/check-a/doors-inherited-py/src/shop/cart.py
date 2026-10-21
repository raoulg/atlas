from shop._base import Base


class Cart(Base):
    def add(self, price: float) -> None:
        print(price)
