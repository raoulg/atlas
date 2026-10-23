//! The total of the counts some lines hold: a line that holds no count is an error of the program's own type, which
//! `?` hands on across each border, up to the route.

#[derive(Debug)]
pub enum Error {
    NotACount(String),
}

fn parse(line: &str) -> Result<u32, Error> {
    match line.parse::<u32>() {
        Ok(n) => Ok(n),
        Err(_) => Err(Error::NotACount(line.to_string())),
    }
}

fn total(lines: &[&str]) -> Result<u32, Error> {
    let mut sum = 0;
    for line in lines {
        sum += parse(line)?;
    }
    Ok(sum)
}

fn main() -> Result<(), Error> {
    let sum = total(&["1", "2", "x"])?;
    println!("{sum}");
    Ok(())
}
