from fits.model import Failed, Fit, Run


def report(run: Run) -> str:
    match run.outcome:
        case Fit(score=s):
            return f"{run.name}: fit {s}"
        case Failed(reason=why):
            return f"{run.name}: failed: {why}"
