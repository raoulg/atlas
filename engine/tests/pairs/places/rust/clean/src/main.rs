use std::path::Path;

fn main() -> std::io::Result<()> {
    let rows = stock::read_rows(Path::new("data/rows.csv"))?;
    println!("{}", stock::page("rows"));
    stock::save_where_told(&rows)
}
