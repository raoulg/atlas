from calc.ast import Add, Expr, Num


def evaluate(e: Expr) -> int:
    match e:
        case Num(value=v):
            return v
        case Add(left=a, right=b):
            return evaluate(a) + evaluate(b)
