from dataclasses import dataclass


@dataclass
class Meta:
    name: str


@dataclass
class Frame:
    values: list[float]
    info: Meta


def build() -> Frame:
    return Frame([1.0], Meta("raw"))


def main() -> None:
    print(build().values)
