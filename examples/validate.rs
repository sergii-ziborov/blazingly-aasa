//! Linting a document, with stable codes you can build CI on.
//!
//! ```text
//! cargo run --example validate
//! ```

use blazingly_aasa::{validate, DiagnosticCode};

// Four separate mistakes, none of which stops the file from parsing.
const DOCUMENT: &[u8] = br#"{
  "applinks": {
    "details": [
      { "appIDs": ["ABCDE12345.com.example.app"],
        "components": [
          { "?": { "id": "42", "flag": true } },
          { "/": "/sale/*" },
          { "comment": "everything else" }
        ]
      },
      { "components": [{ "/": "/orphan" }] }
    ]
  }
}"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report = validate(DOCUMENT)?;

    // `Display` already carries the help line when there is one.
    for diagnostic in report.diagnostics() {
        println!("{diagnostic}");
    }

    println!();
    println!(
        "errors: {}  warnings: {}",
        report.errors().len(),
        report.warnings().len()
    );
    println!("has_errors: {}", report.has_errors());

    // Gate CI on a specific code rather than on a count.
    if report.contains(DiagnosticCode::UnsupportedQueryPredicate) {
        println!("AASA150 present: a query dictionary in this file is inert");
    }
    Ok(())
}
