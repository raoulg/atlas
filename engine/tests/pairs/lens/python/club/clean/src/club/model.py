"""What a club keeps: its members, their visits, the reports they write and what hangs on its board."""

from dataclasses import dataclass


@dataclass
class Member:
    name: str


@dataclass
class Visit:
    member: str
    day: str


@dataclass
class Report:
    title: str
    author: Member


@dataclass
class Post:
    text: str


@dataclass
class Notice(Post):
    until: str
