//! Bootstrap gate: this crate cannot depend on itself, so it includes the
//! shared rules source directly instead of going through the crate API.

#[path = "src/rules.rs"]
mod rules;

fn main() {
    rules::check();
}
