//! The total of the counts some lines hold: whatever goes wrong is boxed into the language's catch-all error, which `?`
//! hands on as no type of the program's own.

use std::error::Error;

fn parse(line: &str) -> Result<u32, Box<dyn Error>> {
    Ok(line.parse::<u32>()?)
}

fn total(lines: &[&str]) -> Result<u32, Box<dyn Error>> {
    let mut sum = 0;
    for line in lines {
        sum += parse(line)?;
    }
    Ok(sum)
}

fn main() -> Result<(), Box<dyn Error>> {
    let sum = total(&["1", "2", "x"])?;
    println!("{sum}");
    Ok(())
}
