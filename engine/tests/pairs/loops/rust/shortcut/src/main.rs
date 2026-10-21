//! Each station of a network, reported: the stations are the module's own list, not the network's.

pub struct Station {
    pub name: &'static str,
}

const STATIONS: [Station; 2] = [Station { name: "De Bilt" }, Station { name: "Eelde" }];

fn report(station: &Station) {
    println!("{}", station.name);
}

fn report_all() {
    for station in &STATIONS {
        report(station);
    }
}

fn main() {
    report_all();
}
