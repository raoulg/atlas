def prophecy(**_):
    """Marks sketch code that is not built yet; Atlas reads the mark by its name."""

    def mark(item):
        return item

    return mark
