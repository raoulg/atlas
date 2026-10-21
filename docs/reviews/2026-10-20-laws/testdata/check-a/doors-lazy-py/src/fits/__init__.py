__all__ = ["fit"]


def __getattr__(name: str) -> object:
    if name == "fit":
        from fits.core import fit

        return fit
    raise AttributeError(name)
