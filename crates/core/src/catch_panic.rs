use alloc::boxed::Box;
use alloc::string::{String, ToString};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::panic::PanicHookInfo;
use tracing::{instrument, Span};

#[instrument]
pub fn program_panic_catch() {
    let prev_hook = std::panic::take_hook();
    let span = Span::current();
    std::panic::set_hook(Box::new(move |panic_info| {
        span.in_scope(|| {
            panic_hook(panic_info);
            prev_hook(panic_info);
        })
    }));
}

pub fn panic_hook(panic_info: &PanicHookInfo) {
    let payload = panic_info.payload();

    #[allow(clippy::manual_map)]
    let payload = if let Some(s) = payload.downcast_ref::<&str>() {
        Some(&**s)
    } else if let Some(s) = payload.downcast_ref::<String>() {
        Some(s.as_str())
    } else {
        None
    };

    let location = panic_info.location().map(|l| l.to_string());
    let backtrace = Backtrace::capture();
    let note = (backtrace.status() == BacktraceStatus::Disabled)
       .then_some("run with RUST_BACKTRACE=1 environment variable to display a backtrace");

    tracing::error!(
        panic.payload = payload,
        panic.location = location,
        panic.backtrace = tracing::field::display(backtrace),
        panic.note = note,
        "A panic occurred",
    );
}
