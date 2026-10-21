from calc.ast import Add, Expr, Num


def parse(text: str) -> Expr:
    left, plus, right = text.partition("+")
    return Add(parse(left), parse(right)) if plus else Num(int(text))
