def f_assign():
    return 1
def f_return():
    return 1
def f_arg():
    return 1
def f_if():
    return 1
def f_for():
    return [1]


def r() -> int:
    return f_return()


def main() -> int:
    x = f_assign()
    print(f_arg())
    if f_if():
        x += 1
    for y in f_for():
        x += y
    return x + r()
