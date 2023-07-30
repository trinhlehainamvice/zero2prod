use std::fmt::Formatter;

pub fn error_chain_fmt(e: &impl std::error::Error, f: &mut Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    // Retrieve all underlying layers errors
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}