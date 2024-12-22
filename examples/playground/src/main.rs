use bytes::{BufMut, BytesMut};
use futures_util::future::join_all;
use futures_util::{join, StreamExt};
use rand::distributions::Standard;
use rand::prelude::SliceRandom;
use rand::{thread_rng, Rng};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info, info_span, instrument, warn, Instrument, Span};
use tracing_lv::client::{TLAppInfo, TLSubscriberExt};
use tracing_lv::{TLAsyncReadExt, TLAsyncWriteExt, TLStreamExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Ok(tracing_subscriber::registry()
        .tracing_lv_init(
            "http://localhost:8080",
            TLAppInfo::new(
                uuid::uuid!("f7570e13-d331-40f7-8d04-e87530308c05"),
                "Example App",
                "1.0",
            )
            .node_id("NodeId")
            .with_data("brief", "192.168.1.2")
            .with_data("node_name", "TestName")
            .with_data("second_name", "Windows11"),
            app_start,
        )
        .await?)
}

const FLAGS_AUTO_EXPAND: bool = true;

async fn app_start() {
    info!("app_start first event");
    app_init().await;

    info!(level = "info", "info msg");
    warn!(level = "warn", "warn msg");
    error!(level = "error", "error msg");
    join_all([
        tokio::spawn(repeat_do_things().in_current_span()),
        tokio::spawn(test_record_data("record data", 1).in_current_span()),
        tokio::spawn(test_io(1024 * 200).in_current_span()),
        tokio::spawn(test_stream(32).in_current_span()),
    ])
    .await;
    info!("all task done!");
}

#[instrument]
async fn test_stream(count: usize) {
    tokio::time::sleep(Duration::from_secs(4)).await;
    info!("start stream");
    let mut iter1 = futures_util::stream::iter(0..count).instrument_stream("Test Stream");
    while let Some(_) = iter1.next().await {
        let delay = thread_rng().gen_range(0..1000);
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    info!("stream recv done");
}
#[instrument]
async fn test_io(data_len: usize) {
    let sample_data = thread_rng()
        .sample_iter(Standard)
        .take(data_len)
        .collect::<Vec<u8>>();
    let mut data: BytesMut = sample_data.iter().collect();
    for _ in 0..500 {
        data.put_slice(sample_data.as_slice());
    }

    // let (duplex1, duplex2) = tokio::io::duplex(512);
    let (duplex1, duplex2) = tokio::io::duplex(1024 * 20);
    let data_len = data.len() as _;
    let future1 = async move {
        let duplex1 = duplex1
            .instrument_read("Test Reader", Some(data_len))
            .instrument_write("Test Writer", Some(data_len));
        let (mut read, mut write) = tokio::io::split(duplex1);
        tokio::join!(
            async move {
                loop {
                    let count: usize = thread_rng().gen_range(1024 * 1024..1024 * 1024 * 4);
                    let some_data = data.split_to(count.min(data.len()));
                    write.write_all(&some_data).await.unwrap();
                    if data.is_empty() {
                        write.shutdown().await.unwrap();
                        break;
                    }
                    let delay = thread_rng().gen_range(30..300);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            },
            async move {
                let mut buf = vec![0; 1024 * 1024 * 8];

                loop {
                    let len = read.read(&mut buf).await.unwrap();
                    if len == 0 {
                        break;
                    }
                }
            }
        )
    }
    .instrument(info_span!("client"));
    let future2 = async move {
        let mut duplex2 = duplex2
            .instrument_read("Test Reader", Some(data_len))
            .instrument_write("Test Writer", Some(data_len));
        let mut buf = vec![0; 1024 * 1024 * 8];
        loop {
            let len = duplex2.read(&mut buf).await.unwrap();
            if len == 0 {
                // duplex2.shutdown().await.unwrap();
                break;
            }
            buf[..len].shuffle(&mut thread_rng());
            duplex2.write_all(&buf[..len]).await.unwrap();
            // let delay = thread_rng().gen_range(10..300);
            // tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
    .instrument(info_span!("server"));
    join!(future1, future2);
}

#[instrument]
async fn test_record_data(data: &str, num: usize) {
    info!(data, num, "init field value");
    let span = Span::current();
    info!("record data field!");
    span.record("data", "record data first");
    span.record("data", "record data two");
    span.record("data", "record data three");
    info!("record num field!");
    span.record("num", 1);
    info!("record end!");
}

#[instrument]
async fn repeat_do_things() {
    info!("start repeat_do_things");
    for _ in 0..10 {
        info!("do something");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[instrument]
async fn app_init() {
    info!("app init start");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    info!("app init finished");
}
