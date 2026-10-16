window.TOUR = {
 "decided": {
  "prophecies": [
   {
    "bodies": 1,
    "file": "src/sunspots/cli.py",
    "holds": true,
    "id": "sunspots.cli.run_archived",
    "leaned_on_by": [],
    "line": 32,
    "sealed": null,
    "supersedes": "sunspots.cli.run",
    "veiled": false
   },
   {
    "bodies": 3,
    "file": "src/sunspots/series.py",
    "holds": true,
    "id": "sunspots.series.Archive",
    "leaned_on_by": [],
    "line": 34,
    "sealed": null,
    "supersedes": null,
    "veiled": false
   },
   {
    "bodies": 0,
    "file": "src/sunspots/series.py",
    "holds": true,
    "id": "sunspots.series.Format",
    "leaned_on_by": [],
    "line": 46,
    "sealed": "raoulg/atlas#19",
    "supersedes": null,
    "veiled": false
   },
   {
    "bodies": 0,
    "file": "src/sunspots/series.py",
    "holds": true,
    "id": "sunspots.series.Compression",
    "leaned_on_by": [],
    "line": 51,
    "sealed": "raoulg/atlas#33",
    "supersedes": null,
    "veiled": false
   }
  ],
  "root": "sunspots",
  "seals": [
   {
    "file": "src/sunspots/series.py",
    "id": "sunspots.series.Format",
    "issue": "raoulg/atlas#19",
    "line": 46,
    "state": "closed",
    "title": "A database is a file: the directory made for it serves its data"
   },
   {
    "file": "src/sunspots/series.py",
    "id": "sunspots.series.Compression",
    "issue": "raoulg/atlas#33",
    "line": 51,
    "state": "open",
    "title": "Showcase of the interaction: history, look, notes, annotations, vision and prophecy, drawn from a real subject"
   }
  ]
 },
 "snippet": "@prophecy\nclass Archive:\n    \"\"\"The smoothed months kept as a file: what goes in comes out.\"\"\"\n\n    def __init__(self, path: Path) -> None: ...\n\n    def write(self, frame: pd.DataFrame) -> None: ...\n\n    def read(self) -> pd.DataFrame:\n        raise NotImplementedError\n\n",
 "built": {
  "prophecies": [
   {
    "bodies": 0,
    "file": "src/sunspots/series.py",
    "holds": true,
    "id": "sunspots.series.Format",
    "leaned_on_by": [],
    "line": 48,
    "sealed": null,
    "supersedes": null,
    "veiled": false
   },
   {
    "bodies": 0,
    "file": "src/sunspots/series.py",
    "holds": true,
    "id": "sunspots.series.Compression",
    "leaned_on_by": [],
    "line": 53,
    "sealed": "raoulg/atlas#33",
    "supersedes": null,
    "veiled": false
   }
  ],
  "root": "sunspots",
  "seals": [
   {
    "file": "src/sunspots/series.py",
    "id": "sunspots.series.Compression",
    "issue": "raoulg/atlas#33",
    "line": 53,
    "state": "open",
    "title": "Showcase of the interaction: history, look, notes, annotations, vision and prophecy, drawn from a real subject"
   }
  ]
 },
 "texts": {
  "look": "The page of `atlas serve` goes there when it next asks, within a second while it is open.",
  "shown": "View: {\"route\":\"sunspots.cli.run\"}\nSelected: sunspots.series.trusted\n17 boxes:\n  Settings() (sunspots.settings.Settings)\n  download (sunspots.download.download)\n  load (sunspots.series.load)\n  trusted (sunspots.series.trusted)\n  smooth (sunspots.series.smooth)\n  Trend() (sunspots.plot.Trend)\n  Trend.save (sunspots.plot.Plot.save)\n  run (sunspots.cli.run)\n  $ run --raw --figure --months --minimum (e:sunspots.cli.run)\n  Web(www.sidc.be) (w:Web:www.sidc.be)\n  File(data/raw/sunspots.csv) (w:File:data/raw/sunspots.csv)\n  File(figures/sunspots.png) (w:File:figures/sunspots.png)\n  Settings \u00b7 settings (t:sunspots.settings.Settings@n:sunspots.cli.run/0)\n  DataFrame (t:ext:pandas.DataFrame@n:sunspots.cli.run/2)\n  DataFrame (t:ext:pandas.DataFrame@n:sunspots.cli.run/3)\n  DataFrame \u00b7 frame (t:ext:pandas.DataFrame@n:sunspots.cli.run/4)\n  Trend (t:sunspots.plot.Trend@n:sunspots.cli.run/5)\n17 lines:\n  Settings() \u2192 Settings \u00b7 settings (data) [n:sunspots.cli.run/0>t:sunspots.settings.Settings@n:sunspots.cli.run/0]\n  Web(www.sidc.be) \u2192 download (data) [w:Web:www.sidc.be>n:sunspots.cli.run/1]\n  download \u2192 File(data/raw/sunspots.csv) (data) [n:sunspots.cli.run/1>w:File:data/raw/sunspots.csv]\n  download \u2192 run (hands back) [n:sunspots.cli.run/1>o:sunspots.cli.run]\n  File(data/raw/sunspots.csv) \u2192 load (data) [w:File:data/raw/sunspots.csv>n:sunspots.cli.run/2]\n  load \u2192 DataFrame (data) [n:sunspots.cli.run/2>t:ext:pandas.DataFrame@n:sunspots.cli.run/2]\n  Settings \u00b7 settings \u2192 trusted (data) [t:sunspots.settings.Settings@n:sunspots.cli.run/0>n:sunspots.cli.run/3]\n  DataFrame \u2192 trusted (data) [t:ext:pandas.DataFrame@n:sunspots.cli.run/2>n:sunspots.cli.run/3]\n  trusted \u2192 DataFrame (data) [n:sunspots.cli.run/3>t:ext:pandas.DataFrame@n:sunspots.cli.run/3]\n  DataFrame \u2192 smooth (data) [t:ext:pandas.DataFrame@n:sunspots.cli.run/3>n:sunspots.cli.run/4]\n  smooth \u2192 DataFrame \u00b7 frame (data) [n:sunspots.cli.run/4>t:ext:pandas.DataFrame@n:sunspots.cli.run/4]\n  Settings \u00b7 settings \u2192 Trend() (data) [t:sunspots.settings.Settings@n:sunspots.cli.run/0>n:sunspots.cli.run/5]\n  Trend() \u2192 Trend (data) [n:sunspots.cli.run/5>t:sunspots.plot.Trend@n:sunspots.cli.run/5]\n  DataFrame \u00b7 frame \u2192 Trend.save (data) [t:ext:pandas.DataFrame@n:sunspots.cli.run/4>n:sunspots.cli.run/6]\n  Trend \u2192 Trend.save (data) [t:sunspots.plot.Trend@n:sunspots.cli.run/5>n:sunspots.cli.run/6]\n  Trend.save \u2192 File(figures/sunspots.png) (data) [n:sunspots.cli.run/6>w:File:figures/sunspots.png]\n  $ run --raw --figure --months --minimum \u2192 run (data) [e:sunspots.cli.run>o:sunspots.cli.run]",
  "notes": "Note 1 on trusted (sunspots.series.trusted), written at 59b48f1 in the view {\"route\":\"sunspots.cli.run\"}:\n  Why a minimum of 1? What happens to the months with no observations?\n  Its place: http://127.0.0.1:51567/#v=%7B%22route%22%3A%22sunspots.cli.run%22%7D&at=sunspots.series.trusted",
  "resolve": "Note 1 on trusted is resolved; the page marks it so while it is served. Tell the person you work with what you did.",
  "annotate": "The page of `atlas serve` shows 1 annotation within a few seconds.",
  "vision": "Envisioned: Keep the smoothed months in a file. The page of `atlas serve` draws it beside the working tree within a second; `look` points at what you mean.\n\nworking tree..vision\n\n+ CLI \u00b7 run-archived\n    + place reads data/raw/sunspots.csv\n    + place writes figures/sunspots.png\n\nfindings raised\n  nothing \u00b7 src/sunspots/series.py:37  Archive.write gives nothing back, and Atlas sees no change and no place it reaches\n",
  "prophetize": "Decided: Keep the smoothed months in a file. Written: src/sunspots/cli.py, src/sunspots/series.py. Marked as prophecies: sunspots.cli.run_archived, sunspots.series.Archive. The files are left for you to commit; the vision is retracted."
 }
};
