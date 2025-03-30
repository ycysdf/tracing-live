// use crate::tokio::{POLL_RESULT, VALUE};
use futures_util::{Sink, Stream, TryStream};
use pin_project::pin_project;
use std::any::type_name;
use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{Span, info, info_span};
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
                info!(item = ?n,"stream item");
                // me.span.record(VALUE, n.as_ref().map(debug));
            }
            Poll::Pending => {
                // me.span.record(POLL_RESULT, debug(&poll));
            }
        }
        poll
    }
}

#[pin_project]
pub struct TLInstrumentedSink<T> {
    #[pin]
    inner: T,
    span: Span,
}

impl<T, I> Sink<I> for TLInstrumentedSink<T>
where
    I: Debug,
    T: Sink<I>,
    T::Error: Debug,
{
    type Error = T::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let me = self.project();
        let _entered = me.span.enter();
        let poll = me.inner.poll_ready(cx);
        info!(?poll, "poll ready");
        poll
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let me = self.project();
        let _entered = me.span.enter();
        info!(?item, "start send");
        let poll = me.inner.start_send(item);
        poll
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let me = self.project();
        let _entered = me.span.enter();
        let poll = me.inner.poll_flush(cx);
        info!(?poll, "poll_flush");
        poll
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let me = self.project();
        let _entered = me.span.enter();
        let poll = me.inner.poll_ready(cx);
        info!(?poll, "poll_flush");
        poll
    }
}

pub trait TLStreamInstrumentExt: Sized {
    fn instrument_stream(self, name: &'static str) -> TLInstrumentedStream<Self>;
}

impl<T> TLStreamInstrumentExt for T
where
    T: Stream<Item: Debug>,
{
    fn instrument_stream(self, name: &'static str) -> TLInstrumentedStream<Self> {
        TLInstrumentedStream {
            inner: self,
            span: info_span!(
                "[t:Stream]",
                item_type = type_name::<T::Item>(),
                // "type" = type_name::<T>(),
                name,
                // value = 0,
                // poll_result = ""
            ),
        }
    }
}

pub trait TLSinkInstrumentExt<I>: Sized {
    fn instrument_sink(self, name: &'static str) -> TLInstrumentedSink<Self>;
}

impl<T, I> TLSinkInstrumentExt<I> for T
where
    I: Debug,
    T: Sink<I>,
    T::Error: Debug,
{
    fn instrument_sink(self, name: &'static str) -> TLInstrumentedSink<Self> {
        TLInstrumentedSink {
            inner: self,
            span: info_span!(
                "[t:Sink]",
                item_type = type_name::<I>(),
                // "type" = type_name::<T>(),
                name,
                // value = 0,
                // poll_result = ""
            ),
        }
    }
}
