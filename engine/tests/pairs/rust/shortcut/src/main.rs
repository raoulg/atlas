//! spend: keeps a ledger of expenses and reports them in one currency.

mod config;
mod expense;
mod rates;
mod report;
mod store;

use std::env;
use std::process::Command;

use store::Store;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first() {
        Some(kind) => run(kind, &args[1..]),
        None => eprintln!("usage: spend import <file> | rates | report [md|csv] [--open]"),
    }
}

/// Runs one command of the command line with its arguments.
fn run(kind: &str, args: &[String]) {
    let config = config::load();
    let store = Store::open();
    match kind {
        "import" => {
            let expenses = expense::read_csv(&args[0]);
            store.save(&expenses);
            println!("imported {} expenses", expenses.len());
        }
        "rates" => {
            let base = &config["base"];
            store.save_rates(base, rates::fetch(&config, base));
        }
        "report" => {
            rates::remember(store.rates(&config["base"]));
            let format = args.first().map(String::as_str).unwrap_or("md");
            let path = report::write(&store, format, &config["title"]);
            println!("wrote {}", path.display());
            if args.iter().any(|a| a == "--open") {
                Command::new("open").arg(&path).status().unwrap();
            }
        }
        _ => eprintln!("unknown command {kind}"),
    }
    if kind != "report" {
        println!("the ledger holds {} expenses", store.rows().len());
    }
}
