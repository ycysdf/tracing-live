#![allow(unused_imports)]

use anyhow::anyhow;
use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{Debug, Formatter};
use std::time::Duration;
use tracing::{info, info_span, instrument, warn, Instrument, Span};
use tracing_lv::TLAppInfo;
use tracing_lv::TLSubscriberExt;
use tracing_lv::FLAGS_AUTO_EXPAND;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .tracing_lv_init(
            "http://localhost:8080",
            TLAppInfo::new(
                uuid::uuid!("f7570e13-d331-40f7-8d04-e87530308c01"), // App Identify
                "Case Test App",                                     // App Name
                "1.0", // App Version. Code Changed. this value should change
            )
            .node_id("TestNode") // Node Identify
            .with_data("brief", "IP")
            .with_data("node_name", "Node Name")
            .with_data("second_name", "Cur Os Name"),
            Default::default(),
            app_main,
        )
        .await?
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct TestData {
    name: String,
    age: usize,
    data: Vec<u8>,
    fff: String,
}
struct DebugPretty<T>(T);
impl<T> Debug for DebugPretty<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#?}", &self.0)
    }
}
async fn app_main() -> anyhow::Result<()> {
    info!(
        a = 1,
        b = 2,
        c = 3,
        d = 5,
        fda = 123,
        fdasfsaafds = 123123132,
        FFFSDF = "LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE",
        BIG = "LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUELONG VALUE LONG VALUE LONG VALUE LONG VALUE LONG VALUE",
        "Many Fields"
    );
    let object = TestData {
        name: "This Is Name".to_string(),
        age: 120,
        data: (0..10)
            .into_iter()
            .map(|n: usize| (n % u8::MAX as usize) as u8)
            .collect(),
        fff: "".to_string(),
    };

    info!(value=?DebugPretty(object),"complex field value");
    info_span!("empty span");
    info_span!("test span").in_scope(|| {
        info!("FIRST EVENT");
        info!("END EVENT");
    });

    let _ = test_nest_error(0);

    let _ = with_result1();
    let _ = with_result2();
    let _ = with_err1();
    let _ = with_err2();
    let _ = with_result_and_err1();
    let _ = with_result_and_err2();

    test_auto_expand().await;

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    test_empty_children().await;

    tokio::spawn(
        async move {
            info!("EVENT");
        }
        .instrument(info_span!("spawn task")),
    );
    {
        let span = info_span!("spawn_blocking task");
        tokio::task::spawn_blocking(move || {
            let _entered = span.enter();
            info!("EVENT");
        })
        .await
        .unwrap();
    }

    tracing::info!("Test thread");
    std::thread::spawn(|| {
        hello();
    })
    .join()
    .unwrap();
    tracing::info!("Test thread JOIN END");

    tracing::info!("Test spawn_blocking");
    tokio::task::spawn_blocking(|| {
        hello();
    })
    .await
    .unwrap();
    tracing::info!("Test spawn_blocking END");

    let span = { info_span!("DoThingsContainer") };
    test_saved(span).await;

    info!("{}", "Long Event.".repeat(1024));

    many_event();
    Ok(())
}

#[instrument(ret, err)]
fn test_nest_error(i: usize) -> anyhow::Result<String> {
    if i == 6 {
        return Err(anyhow!("Source Error"));
    }
    test_nest_error(i + 1)
}

#[instrument(ret)]
fn with_result1() -> anyhow::Result<String> {
    anyhow::Ok("Return Ok Value".into())
}

#[instrument(ret)]
fn with_result2() -> anyhow::Result<String> {
    info!("EVENT1");
    info!("EVENT2");
    Err(anyhow!("Return Err"))
}

#[instrument(err)]
fn with_err1() -> anyhow::Result<String> {
    anyhow::Ok("Return Ok Value".into())
}

#[instrument(err(Debug))]
fn with_err2() -> anyhow::Result<String> {
    Err(anyhow!("Return Err"))
}
#[instrument(ret, err)]
fn with_result_and_err1() -> anyhow::Result<String> {
    anyhow::Ok("Return Ok Value".into())
}

#[instrument(ret, err)]
fn with_result_and_err2() -> anyhow::Result<String> {
    Err(anyhow!("Return Err"))
}

#[instrument]
async fn test_auto_expand() {
    tokio::time::sleep(Duration::from_secs(6)).await;
    info_span!("TEST", FLAGS_AUTO_EXPAND).in_scope(|| info!("EVENT"));
}
fn hello() {
    tracing::info!("Hello, world!");
}

#[instrument]
async fn test_empty_children() {
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    info!("EVENT");
}

#[instrument]
async fn test_saved(container: Span) {
    let mut tasks: HashMap<String, VecDeque<usize>> = HashMap::new();
    tasks.insert("A".into(), (0..10).into_iter().collect());
    tasks.insert("B".into(), (10..20).into_iter().collect());
    tasks.insert("C".into(), (20..30).into_iter().collect());
    info!("test_saved start ");
    let mut map = HashMap::new();
    loop {
        let key = tasks
            .keys()
            .nth(rand::thread_rng().gen_range(0..tasks.len()))
            .unwrap()
            .clone();
        let nums = tasks.get_mut(&key).unwrap();
        let num = nums.pop_front();
        let Some(num) = num else {
            tasks.remove(&key);
            if tasks.is_empty() {
                break;
            }
            continue;
        };
        async {
            let item = map
                .entry(key.clone())
                .or_insert_with(|| info_span!("SavedSpan", key))
                .clone();
            async move {
                async move {
                    info!(num, "do something start");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1))
                        .instrument(info_span!("do something"))
                        .await;
                    info!("do something end");
                }
                .instrument(info_span!("inner span", num))
                .await;
            }
            .instrument(item)
            .await
        }
        .instrument(container.clone())
        .await;

        info!("Other EVENT1");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        info!("Other EVENT2");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        info!("Other EVENT3");
    }
    info!("test_saved end ");
}

#[instrument]
fn many_event() {
    for i in 0..1024 * 2 {
        info!("event {}", i);
    }
}
