"""Run a statement the command line names: which way it goes the code does not say."""

import sqlite3
import sys


def run() -> None:
    con = sqlite3.connect("data/app.db")
    print(con.execute(sys.argv[1]).fetchall())
