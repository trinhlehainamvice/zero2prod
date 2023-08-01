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

// Some tasks are CPU-intensive, they should be handled in separate threads to avoid blocking event loop thread
pub fn spawn_blocking_task_with_tracing<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // Spawn a new thread to handle this task, also new span will be created in this new thread
    // Need to pass span of thread than spawned task to block thread, for that thread can subscribe to parent span
    let current_span = tracing::Span::current();
    // Hash password algorithm consuming a lot of CPU power, may cause blocking event loop thread handle current request
    // spawn blocking task to another thread to let current event loop thread to continue to process non-blocking tasks (another requests)
    tokio::task::spawn_blocking(move || current_span.in_scope(f))
}
