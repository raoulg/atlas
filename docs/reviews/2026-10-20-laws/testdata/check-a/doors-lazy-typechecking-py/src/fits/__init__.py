from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from fits.core import fit

__all__ = ["fit"]


def __getattr__(name: str) -> object:
    if name == "fit":
        from fits.core import fit

        return fit
    raise AttributeError(name)
