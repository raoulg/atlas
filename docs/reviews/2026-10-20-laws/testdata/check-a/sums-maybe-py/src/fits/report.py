from fits.model import Fit, Result


def report(r: Result) -> str:
    return f"fit {r.score}" if isinstance(r, Fit) else "nothing"
