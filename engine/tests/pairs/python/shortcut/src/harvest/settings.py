"""How a run is configured: a JSON file."""

import json

CONFIG_FILE = "harvest.json"


def load_settings() -> dict:
    with open(CONFIG_FILE) as f:
        return json.load(f)
