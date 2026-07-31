use std::io::{self, Read as _};
use std::process::ExitCode;

use open_gpui_canvas_jellyflow::smoke_receipt::{SmokeRunReceiptV1, schema_json};

const USAGE: &str = "usage: smoke_receipt_contract <schema|validate>";

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let operation = arguments.next();
    if arguments.next().is_some() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    match operation.as_deref() {
        Some("schema") => {
            println!("{}", schema_json());
            ExitCode::SUCCESS
        }
        Some("validate") => validate_stdin(),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn validate_stdin() -> ExitCode {
    let mut input = Vec::new();
    if let Err(error) = io::stdin().read_to_end(&mut input) {
        eprintln!("failed to read receipt: {error}");
        return ExitCode::FAILURE;
    }
    let receipt = match SmokeRunReceiptV1::from_json_slice(&input) {
        Ok(receipt) => receipt,
        Err(error) => {
            eprintln!("invalid receipt JSON: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = receipt.validate() {
        eprintln!("invalid receipt semantics: {error}");
        return ExitCode::FAILURE;
    }
    match serde_json::to_string(&receipt) {
        Ok(canonical) => {
            println!("{canonical}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to serialize canonical receipt: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_operation_is_valid_json() {
        serde_json::from_str::<serde_json::Value>(&schema_json()).expect("schema JSON");
    }
}
