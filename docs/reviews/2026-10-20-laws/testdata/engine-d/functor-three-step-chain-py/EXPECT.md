Route: `chain.run`. pipeline = summarize . parse . load.
Closed `pipeline` (run/0): domain File(data/in.txt), File(data/stop.txt); codomain Report, File(data/report.txt); Raw and Parsed internal — "the domain of a composite is the domain of its first arrows", "an object between two arrows of a composite is internal to it".
Opened: load: in.txt -> Raw; parse: Raw, stop.txt -> Parsed; summarize: Parsed -> Report, report.txt; pipeline's own arrow: Report -> Report (it returns summarize's result).
Observed (0.68.0): closed holds. Opened, the own pill `o:chain.run/0` takes Raw, Parsed and Report (from `made_from`, flow.rs:689-702 via viewer/index.html:1237): two lines (Raw, Parsed -> pipeline's own arrow) that no data path has. Minor.
