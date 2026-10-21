from dataclasses import dataclass


@dataclass
class Message:
    text: str


def send(m: Message) -> int:
    return len(m.text)


def two_calls(c: bool) -> int:
    msg = Message("on") if c else Message("off")
    return send(msg)


def one_call(c: bool) -> int:
    msg = Message("on" if c else "off")
    return send(msg)


def handed_on(c: bool) -> int:
    return send(Message("on") if c else Message("off"))


def main() -> int:
    return two_calls(True) + one_call(True) + handed_on(True)
