"""How a run is configured: a TOML file, read into typed settings."""

import tomllib
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

from harvest.model import Unit


class ReportFormat(Enum):
    JSON = "json"
    CSV = "csv"


@dataclass(frozen=True)
class Settings:
    report_format: ReportFormat = ReportFormat.JSON
    unit: Unit = Unit.CELSIUS
    timeout: float = 10.0


def load_settings(path: Path) -> Settings:
    """The settings a TOML file holds; what it leaves out keeps its default."""
    with path.open("rb") as f:
        table = tomllib.load(f)
    defaults = Settings()
    return Settings(
        report_format=ReportFormat(table.get("report_format", defaults.report_format.value)),
        unit=Unit(table.get("unit", defaults.unit.value)),
        timeout=float(table.get("timeout", defaults.timeout)),
    )
