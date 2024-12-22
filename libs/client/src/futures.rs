use crate::tokio::{POLL_RESULT, VALUE};
use futures_util::Stream;
use pin_project::pin_project;
use std::any::type_name;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{info_span, Span};
use tracing_core::field::debug;

#[pin_project]
pub struct TLInstrumentedStream<T> {
    #[pin]
    inner: T,
    span: Span,
}

impl<T> Stream for TLInstrumentedStream<T>
where
    T: Stream<Item: Debug>,
{
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.project();
        let _entered = me.span.enter();
        let poll = me.inner.poll_next(cx);
        match &poll {
            Poll::Ready(n) => {
                me.span.record(VALUE, n.as_ref().map(debug));
            }
            Poll::Pending => {
                me.span.record(POLL_RESULT, debug(&poll));
            }
        }
        poll
    }
}

pub trait TLStreamExt: Sized {
    fn instrument_stream(self, name: &'static str) -> TLInstrumentedStream<Self>;
}

impl<T> TLStreamExt for T
where
    T: Stream,
{
    fn instrument_stream(self, name: &'static str) -> TLInstrumentedStream<Self> {
        TLInstrumentedStream {
            inner: self,
            span: info_span!(
                "[t:Stream]",
                item_type = type_name::<T::Item>(),
                "type" = type_name::<T>(),
                name,
                value = 0,
                poll_result = ""
            ),
        }
    }
}
