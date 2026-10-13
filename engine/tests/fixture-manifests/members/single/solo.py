"""A package of one module."""


def tune(pitch: float) -> float:
    return _octave(pitch)


def _octave(pitch: float) -> float:
    return pitch * 2
