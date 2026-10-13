"""Figures of a sunspot series: each kind of plot draws a figure, and every plot saves one."""

from abc import ABC, abstractmethod
from pathlib import Path

import pandas as pd
from matplotlib.figure import Figure

from sunspots.settings import Settings


class Plot(ABC):
    """A figure drawn from a frame and saved as an image."""

    def __init__(self, settings: Settings) -> None:
        self.settings = settings

    @abstractmethod
    def draw(self, frame: pd.DataFrame) -> Figure: ...

    def save(self, frame: pd.DataFrame, to: Path) -> Path:
        """Draw `frame` and write the figure to `to`; returns `to`."""
        figure = self.draw(frame)
        to.parent.mkdir(parents=True, exist_ok=True)
        figure.savefig(to, dpi=self.settings.dpi)
        return to


class Trend(Plot):
    """The monthly means as a thin line, with their smoothed mean over them."""

    def draw(self, frame: pd.DataFrame) -> Figure:
        figure = Figure(figsize=self.settings.size, layout="constrained")
        ax = figure.add_subplot()
        ax.plot(frame.index, frame["mean"], color="0.7", linewidth=0.6, label="monthly mean")
        ax.plot(frame.index, frame["smoothed"], color="C0", label=f"{self.settings.months}-month mean")
        ax.set_title(self.settings.title)
        ax.set_ylabel("sunspot number")
        ax.legend()
        return figure
