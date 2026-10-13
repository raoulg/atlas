"""Sunspots: SILSO's monthly mean sunspot number, downloaded, smoothed and drawn."""

from sunspots.download import download
from sunspots.plot import Plot, Trend
from sunspots.series import load, smooth
from sunspots.settings import Settings

__all__ = ["Plot", "Settings", "Trend", "download", "load", "smooth"]
