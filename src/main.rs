mod amount;
mod engine;
mod prelude;

use prelude::*;
use std::env;
use std::fs::File;
use std::io;

fn main() -> Result<()> {
    // skip the first arg as that's the binary path
    let file = if let Some(csv_path) = env::args().nth(1) {
        File::open(csv_path).context("cannot open csv file")
    } else {
        Err(anyhow!("no input file path provided"))
    }?;

    // processes all transactions in the file into a map of client ids to
    // states
    let clients = engine::read_transactions(file)?;

    // outputs the client state in csv format
    engine::write_clients(io::stdout(), clients)?;

    Ok(())
}
