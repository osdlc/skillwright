//! String helpers shared across the crate.

use std::fmt::Display;

/// `a`, `a <conjunction> b`, `a, b <conjunction> c`.
pub(crate) fn list(items: impl IntoIterator<Item = impl Display>, conjunction: &str) -> String {
    let items: Vec<String> = items.into_iter().map(|item| item.to_string()).collect();
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} {conjunction} {last}", rest.join(", ")),
    }
}

/// `` `a`, `b` and `c` `` — for a refusal that has to name what it found.
pub(crate) fn named(names: impl IntoIterator<Item = impl Display>) -> String {
    list(names.into_iter().map(|name| format!("`{name}`")), "and")
}
