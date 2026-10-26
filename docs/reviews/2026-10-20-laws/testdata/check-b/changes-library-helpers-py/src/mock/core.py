import bisect
import heapq
from dataclasses import dataclass


@dataclass(order=True)
class Job:
    rank: int


def by_append(jobs: list[Job], j: Job) -> int:
    jobs.append(j)
    return len(jobs)


def by_heappush(jobs: list[Job], j: Job) -> int:
    heapq.heappush(jobs, j)
    return len(jobs)


def by_insort(jobs: list[Job], j: Job) -> int:
    bisect.insort(jobs, j)
    return len(jobs)


def main() -> int:
    return by_append([], Job(1)) + by_heappush([], Job(1)) + by_insort([], Job(1))
