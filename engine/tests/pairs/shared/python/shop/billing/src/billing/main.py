from pathlib import Path

import notekit
from notekit import disk


def main() -> None:
    bills = Path("data/bills.csv").read_text()
    notekit.save("out/bills.txt", bills)
    notekit.write("out/bills.log", "saved")
    disk.sync("out/bills.log")
