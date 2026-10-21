import contextlib


def f_attr():
    return "a"
def f_with():
    return contextlib.nullcontext(1)
def f_binop():
    return 1
def f_statement():
    return 1
def f_underscore():
    return 1


def main() -> int:
    x = len(f_attr().upper())
    with f_with() as c:
        x += c
    f_statement()
    _ = f_underscore()
    return x + (f_binop() + 1)
