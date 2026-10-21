class A:
    def f(self) -> int:
        return 1


class X:
    pass


class C(X):
    def f(self) -> str:
        return "c"


class B(A):
    pass


class D(B, C):
    def f(self) -> str:
        return "d"
