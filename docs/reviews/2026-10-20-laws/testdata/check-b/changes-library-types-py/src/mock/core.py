import collections
import io
import queue


def to_buffer(buf: io.StringIO, s: str) -> None:
    buf.write(s)


def to_queue(q: queue.Queue[int]) -> None:
    q.put(1)


def to_deque(d: collections.deque[int]) -> None:
    d.append(1)


def main() -> int:
    buf = io.StringIO()
    to_buffer(buf, "x")
    q: queue.Queue[int] = queue.Queue()
    to_queue(q)
    d: collections.deque[int] = collections.deque()
    to_deque(d)
    return len(buf.getvalue()) + q.qsize() + len(d)
