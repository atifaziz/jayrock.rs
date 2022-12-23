use std::{error::Error, io::Read};

use jayrock::json::*;

fn main() -> Result<(), Box<dyn Error>> {
    let mut json = String::new();
    _ = std::io::stdin().read_to_string(&mut json)?;
    for token in JsonTextReader::new(json.as_str()) {
        println!("{token:?}");
    }
    Ok(())
}
