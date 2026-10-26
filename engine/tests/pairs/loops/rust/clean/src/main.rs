//! Each station of a network, reported: the stations come from the network the route is handed.

pub struct Station {
    pub name: String,
}

pub struct Network {
    pub stations: Vec<Station>,
}

fn load(path: &str) -> Network {
    let text = std::fs::read_to_string(path).unwrap();
    Network { stations: text.lines().map(|l| Station { name: l.to_string() }).collect() }
}

fn report(station: &Station) {
    println!("{}", station.name);
}

fn report_all(network: &Network) {
    for station in &network.stations {
        report(station);
    }
}

fn main() {
    let network = load("stations.txt");
    report_all(&network);
}
