from notes.keep import save_log, save_report, show_report


def main() -> None:
    save_report("all well")
    save_log("ran")
    print(show_report())
