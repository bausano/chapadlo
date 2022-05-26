mod amount;
mod engine;
mod prelude;

use std::env;
use std::error::Error;
use std::fs::File;
use std::io;

fn main() -> Result<(), Box<dyn Error>> {
    // skip the first arg as that's the binary path
    let file = if let Some(csv_path) = env::args().skip(1).next() {
        File::open(csv_path).map_err(|_| "cannot open csv file")
    } else {
        Err("no input file path provided")
    }?;

    // processes all transactions in the file into a map of client ids to
    // states
    let clients = engine::read_transactions(file)?;

    // outputs the client state in csv format
    engine::write_clients(io::stdout(), clients)?;

    Ok(())
}
