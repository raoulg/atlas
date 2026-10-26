fn main() -> std::io::Result<()> {
    let rows = stock::default_rows()?;
    println!("{}", stock::page("rows")?);
    stock::save_where_told(&rows)
}
