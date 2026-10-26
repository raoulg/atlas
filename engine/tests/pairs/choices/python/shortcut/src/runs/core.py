"""A job run in one of two modes, and a line that says how it went."""

from dataclasses import dataclass
from enum import Enum


class Level(Enum):
    QUIET = "quiet"
    LOUD = "loud"


@dataclass
class Settings:
    mode: str
    level: Level


@dataclass
class Job:
    name: str
    steps: list[str]


@dataclass
class Message:
    text: str


class Runner:
    def __init__(self, mode: str) -> None:
        self.mode = mode

    def batch(self) -> int:
        return 64 if self.mode == "parallel" else 1

    def workers(self) -> int:
        return 8 if self.mode == "parallel" else 1


def retries(mode: str) -> int:
    return 3 if mode == "parallel" else 1


def timeout(mode: str) -> int:
    return 60 if mode == "parallel" else 600


def verbosity(settings: Settings) -> int:
    m = settings.mode
    shown = 3 if m == "parallel" else 1
    return shown + (1 if m == "parallel" else 0)


def prefix(level: Level) -> str:
    return "!" if level is Level.LOUD else ""


def suffix(level: Level) -> str:
    match level:
        case Level.LOUD:
            return "!"
        case _:
            return "."


def load(name: str) -> Job:
    return Job(name, [name])


def infer(name: str) -> Job:
    return Job(name, [])


def finished(job: Job, mode: str) -> bool:
    return len(job.steps) * Runner(mode).batch() <= timeout(mode)


def send(message: Message) -> int:
    print(message.text)
    return len(message.text)


def run(name: str, settings: Settings, cached: bool) -> int:
    job = load(name) if cached else infer(name)
    done = finished(job, settings.mode)
    text = prefix(settings.level) + job.name + (" done" if done else " failed") + suffix(settings.level)
    return send(Message(text)) + retries(settings.mode) + verbosity(settings)
