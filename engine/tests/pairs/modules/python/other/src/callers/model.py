"""Classes called as functions. Only a torch module's call runs its forward."""

from jaxtyping import Float
from torch import Tensor, nn


class Scaler:
    """No module: its call is its own `__call__`, whatever else it names `forward`."""

    def forward(self, x: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch dim"]:
        return x

    def __call__(self, x: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch dim"]:
        return x * 2.0


class Gate(nn.Module):
    """A module with a call of its own: its call runs that, not its forward."""

    def forward(self, x: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch dim"]:
        return x

    def __call__(self, x: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch dim"]:
        return self.forward(x) * 0.5


class Doubled(Gate):
    """Below a module with a call of its own: its call runs that call, which runs this forward."""

    def forward(self, x: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch dim"]:
        return x * 2.0


def run(scaler: Scaler, gate: Gate, doubled: Doubled, x: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch dim"]:
    return doubled(gate(scaler(x)))


def show(scaler: Scaler, gate: Gate) -> None:
    """Instances handed on, not called: printing a module runs its `__repr__`, neither its call nor its forward."""
    print(scaler)
    print(gate)
