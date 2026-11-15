//! Embedding example: draw the signup dialog and print its result.
//!
//! Run it from a real terminal (it needs a console device):
//!
//! ```sh
//! cargo run --example library
//! ```

use wbat64::{Dialog, Layout, Outcome};

fn main() -> wbat64::Result<()> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/dialogs.txt");
    let layout = Layout::from_file(path, "signup")?;
    let result = Dialog::new(layout)
        .var("city", "Berlin")
        .var("WCB2", "1")
        .run()?;

    match &result.outcome {
        Outcome::Button { index, name } => println!("button {index} ({name})"),
        Outcome::Escape => println!("cancelled"),
        Outcome::Timeout { index, .. } => println!("timeout (button {index:?})"),
    }
    println!("name = {:?}", result.inputs.get("name"));
    println!("city = {:?}", result.inputs.get("city"));
    println!("checkboxes = {:?}", result.checkboxes);
    println!("radio = {:?}", result.radio);
    Ok(())
}
