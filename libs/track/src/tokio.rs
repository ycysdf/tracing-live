use pin_project::pin_project;
use std::any::type_name;
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};
use tracing::{info_span, Span};
use tracing_core::field::debug;

pub const VALUE: &'static str = "value";
pub const INCREMENTAL: &'static str = "incremental";
pub const WANT_WRITE: &'static str = "want_write";
pub const POLL_RESULT: &'static str = "poll_result";

#[pin_project]
pub struct TLInstrumentedAsyncRead<T> {
    #[pin]
    inner: T,
    span: Span,
    total_read: u64,
}
impl<T> AsyncWrite for TLInstrumentedAsyncRead<T>
where
    T: AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.project().inner.poll_shutdown(cx)
    }
}

impl<T> AsyncBufRead for TLInstrumentedAsyncRead<T>
where
    T: AsyncBufRead,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        self.project().inner.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().inner.consume(amt)
    }
}

impl<T> AsyncRead for TLInstrumentedAsyncRead<T>
where
    T: AsyncRead,
{
    #[track_caller]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let me = self.project();
        let entered = me.span.enter();
        let prev_len = buf.filled().len();
        let r = me.inner.poll_read(cx, buf);
        if r.is_pending() {
            return r;
        }
        let filled_len = buf.filled().len() - prev_len;
        *me.total_read = *me.total_read + filled_len as u64;

        let identifier = me.span.metadata().unwrap().callsite();
        let value_field = me.span.field(VALUE).unwrap();
        let incremental_field = me.span.field(INCREMENTAL).unwrap();
        let poll_result_field = me.span.field(POLL_RESULT).unwrap();
        me.span.record_all(
            &tracing_core::field::FieldSet::new(&[VALUE, INCREMENTAL, POLL_RESULT], identifier)
                .value_set(&[
                    (
                        &value_field,
                        Some(&*me.total_read as &(dyn tracing_core::field::Value)),
                    ),
                    (
                        &incremental_field,
                        Some(&match &r {
                            Poll::Ready(Ok(_)) => Some(filled_len),
                            _ => None,
                        } as &(dyn tracing_core::field::Value)),
                    ),
                    (
                        &poll_result_field,
                        Some(&debug(&r) as &(dyn tracing_core::field::Value)),
                    ),
                ]),
        );
        drop(entered);
        r
    }
}

pub trait TLAsyncReadExt: Sized {
    fn instrument_read(
        self,
        name: &'static str,
        total_size: Option<u64>,
    ) -> TLInstrumentedAsyncRead<Self>;
}

impl<T> TLAsyncReadExt for T
where
    T: AsyncRead,
{
    fn instrument_read(
        self,
        name: &'static str,
        total_size: Option<u64>,
    ) -> TLInstrumentedAsyncRead<Self> {
        TLInstrumentedAsyncRead {
            inner: self,
            span: info_span!(
                "[t:AsyncRead]",
                name,
                "type" = type_name::<T>(),
                value = 0,
                incremental = 0,
                poll_result = "",
                total_size
            ),
            total_read: 0,
        }
    }
}

#[pin_project]
pub struct TLAsyncWrite<T> {
    #[pin]
    inner: T,
    span: Span,
    total_write: u64,
}
impl<T> AsyncRead for TLAsyncWrite<T>
where
    T: AsyncRead,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}
impl<T> AsyncBufRead for TLAsyncWrite<T>
where
    T: AsyncBufRead,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        self.project().inner.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().inner.consume(amt)
    }
}

impl<T> AsyncWrite for TLAsyncWrite<T>
where
    T: AsyncWrite,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        let me = self.project();
        let entered = me.span.enter();
        let r = me.inner.poll_write(cx, buf);
        if r.is_pending() {
            return r;
        }
        if let Poll::Ready(Ok(count)) = &r {
            *me.total_write = *me.total_write + *count as u64;
        }
        let identifier = me.span.metadata().unwrap().callsite();
        let value_field = me.span.field(VALUE).unwrap();
        let incremental_field = me.span.field(INCREMENTAL).unwrap();
        let poll_result_field = me.span.field(POLL_RESULT).unwrap();
        let want_write_field = me.span.field(WANT_WRITE).unwrap();
        me.span.record_all(
            &tracing_core::field::FieldSet::new(
                &[VALUE, INCREMENTAL, POLL_RESULT, WANT_WRITE],
                identifier,
            )
            .value_set(&[
                (
                    &value_field,
                    Some(me.total_write as &(dyn tracing_core::field::Value)),
                ),
                (
                    &incremental_field,
                    Some(&match &r {
                        Poll::Ready(Ok(n)) => Some(*n),
                        _ => None,
                    } as &(dyn tracing_core::field::Value)),
                ),
                (
                    &poll_result_field,
                    Some(&debug(&r) as &(dyn tracing_core::field::Value)),
                ),
                (
                    &want_write_field,
                    Some(&buf.len() as &(dyn tracing_core::field::Value)),
                ),
            ]),
        );
        drop(entered);
        r
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let me = self.project();
        let _entered = me.span.enter();
        me.inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let me = self.project();
        let _entered = me.span.enter();
        me.inner.poll_shutdown(cx)
    }
}

pub trait TLAsyncWriteExt: Sized {
    fn instrument_write(self, name: &'static str, total_size: Option<u64>) -> TLAsyncWrite<Self>;
}

impl<T> TLAsyncWriteExt for T
where
    T: AsyncWrite,
{
    fn instrument_write(self, name: &'static str, total_size: Option<u64>) -> TLAsyncWrite<Self> {
        TLAsyncWrite {
            inner: self,
            span: info_span!(
                "[t:AsyncWrite]",
                name,
                "type" = type_name::<T>(),
                value = 0,
                incremental = 0,
                want_write = "",
                poll_result = "",
                total_size
            ),
            total_write: 0,
        }
    }
}
