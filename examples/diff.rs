//! Comparing two documents by effective policy rather than by bytes.
//!
//! ```text
//! cargo run --example diff
//! ```

use blazingly_aasa::CompiledAasa;

// What the site serves.
const ORIGIN: &[u8] = br#"{
  "applinks": {
    "details": [{
      "appIDs": ["ABCDE12345.com.example.app"],
      "defaults": { "caseSensitive": false },
      "components": [{ "/": "/help/*" }]
    }]
  }
}"#;

// The same policy, written differently: the flag moved down into the rule, the keys reordered,
// the whitespace changed. Nothing about matching changed.
const REFORMATTED: &[u8] = br#"{
  "applinks": { "details": [ {
    "components": [ { "caseSensitive": false, "/": "/help/*" } ],
    "appIDs": [ "ABCDE12345.com.example.app" ]
  } ] }
}"#;

// What the CDN is still handing out: an older policy.
const STALE: &[u8] = br#"{
  "applinks": {
    "details": [{
      "appIDs": ["ABCDE12345.com.example.app"],
      "components": [{ "/": "/help/*", "caseSensitive": true }]
    }]
  }
}"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let origin = CompiledAasa::parse(ORIGIN)?;
    let reformatted = CompiledAasa::parse(REFORMATTED)?;
    let stale = CompiledAasa::parse(STALE)?;

    // Reformatting is not a change.
    let cosmetic = origin.semantic_diff(&reformatted);
    println!("origin vs reformatted");
    println!("  equivalent:          {}", cosmetic.is_equivalent());
    println!(
        "  structurally_equal:  {}",
        origin.structural_equal(&reformatted)
    );

    // A moved flag is.
    let real = origin.semantic_diff(&stale);
    println!();
    println!("origin vs stale CDN copy");
    println!("  equivalent: {}", real.is_equivalent());
    for change in real.changes() {
        println!("  {change}");
    }

    // `equivalent == true` guarantees the same decision for every URL. `false` does not prove a
    // difference exists — it means one may, and no witness URL is produced. Here one does:
    println!();
    let url = "https://example.com/HELP/1";
    println!("{url}");
    println!(
        "  origin: {}",
        origin.decide("example.com", "ABCDE12345.com.example.app", url)?
    );
    println!(
        "  stale:  {}",
        stale.decide("example.com", "ABCDE12345.com.example.app", url)?
    );
    Ok(())
}
