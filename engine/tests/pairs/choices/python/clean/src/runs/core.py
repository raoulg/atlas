"""A job run in one of two modes, and a line that says how it went."""

from dataclasses import dataclass
from enum import Enum


class Mode(Enum):
    SERIAL = "serial"
    PARALLEL = "parallel"


@dataclass(frozen=True)
class Plan:
    """What a mode means for a run."""

    batch: int
    workers: int
    retries: int
    timeout: int
    verbosity: int


PLANS: dict[Mode, Plan] = {
    Mode.SERIAL: Plan(batch=1, workers=1, retries=1, timeout=600, verbosity=1),
    Mode.PARALLEL: Plan(batch=64, workers=8, retries=3, timeout=60, verbosity=4),
}


class Level(Enum):
    QUIET = "quiet"
    LOUD = "loud"

    def prefix(self) -> str:
        match self:
            case Level.QUIET:
                return ""
            case Level.LOUD:
                return "!"

    def suffix(self) -> str:
        match self:
            case Level.QUIET:
                return "."
            case Level.LOUD:
                return "!"


@dataclass
class Settings:
    mode: Mode
    level: Level


@dataclass
class Job:
    name: str
    steps: list[str]


@dataclass
class Message:
    text: str


class Runner:
    def __init__(self, mode: Mode) -> None:
        self.plan = PLANS[mode]

    def batch(self) -> int:
        return self.plan.batch

    def workers(self) -> int:
        return self.plan.workers


def load(name: str) -> Job:
    return Job(name, [name])


def infer(name: str) -> Job:
    return Job(name, [])


def source(name: str, cached: bool) -> Job:
    """Where the job comes from: the cache, or what its name says."""
    return load(name) if cached else infer(name)


def finished(job: Job, mode: Mode) -> bool:
    return len(job.steps) * Runner(mode).batch() <= PLANS[mode].timeout


def send(message: Message) -> int:
    print(message.text)
    return len(message.text)


def report(job: Job, level: Level, done: bool) -> int:
    message = Message(level.prefix() + job.name + " done") if done else Message(job.name + " failed" + level.suffix())
    return send(message)


def run(name: str, settings: Settings, cached: bool) -> int:
    job = source(name, cached)
    plan = PLANS[settings.mode]
    return report(job, settings.level, finished(job, settings.mode)) + plan.retries + plan.verbosity
