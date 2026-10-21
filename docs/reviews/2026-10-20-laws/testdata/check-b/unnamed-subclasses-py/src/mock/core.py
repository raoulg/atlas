class Sink:
    def write(self, s: str) -> int:
        return len(s)


class CsvSink(Sink): ...
class JsonSink(Sink): ...


def csv_sink() -> Sink:
    return CsvSink()


def json_sink() -> Sink:
    return JsonSink()


def by_constructors(csv: bool) -> int:
    s = CsvSink() if csv else JsonSink()
    return s.write("x")


def by_factories(csv: bool) -> int:
    s = csv_sink() if csv else json_sink()
    return s.write("x")


def main() -> int:
    return by_constructors(True) + by_factories(True)
