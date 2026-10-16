"""Where a run keeps its data, relative to the directory it runs in, and the service it asks."""

from pathlib import Path

CONFIG_FILE = Path("harvest.toml")
DATA_DIR = Path("data")
STATIONS_FILE = DATA_DIR / "stations.csv"
ARCHIVE_DIR = DATA_DIR / "archive"
DATABASE = DATA_DIR / "harvest.db"
REPORT_DIR = Path("reports")
API_URL = "https://api.weather.example/v1"
