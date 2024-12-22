[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/ycysdf/tracing-live#LICENSE)
[![Crates.io](https://img.shields.io/crates/v/tracing-live.svg)](https://crates.io/crates/tracing-lv)
[![Docs](https://docs.rs/tracing-live/badge.svg)](https://docs.rs/tracing-lv)


# Tracing Live

> In Progress!. 项目正在进行中

基于 Tracing，轻松的实时追踪你的 Rust 程序！

![](/assets/show.png)

## 特征

- 实时 Span、Event、Field 追踪
- 结构化数据
- 多节点、多 App 实例
- `AsyncRead`、`AsyncWrite` 集成

## Start Server

```shell
docker run --name timescaledb -d -p 5432:5432 -e POSTGRES_PASSWORD=<You Posgtres Password> -v <Your Posgtres Data Dir>:/var/lib/postgresql/data timescale/timescaledb:latest-pg16

docker run --name tracing-live-server -d -p 443:443 -p 8080:8080 -e DATABASE_URL="postgresql://postgres:123456@host.docker.internal:5432/postgres" --add-host=host.docker.internal:host-gateway ycysdf/tracing-live-server
```

## Setup Client

dependencies:

```toml
tracing-lv = { version = "0.0.1-beta.3" }
tracing = "0.1"
tokio = { version = "1", features = ["rt", "rt-multi-thread"] }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
```

Simple Sample: 

```rust
use tracing::{info, info_span, warn};
use tracing_lv::client::{TLAppInfo, TLSubscriberExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .tracing_lv_init(
            "http://localhost:8080",
            TLAppInfo::new(
                uuid::uuid!("f7570e13-d331-40f7-8d04-e87530308c05"),
                "Simple App",
                "1.0",
            )
            .node_id("TestNode")
            .with_data("node_name", "TestNode")
            .with_data("brief", "IP")
            .with_data("second_name", "Cur Os Name"),
            app_main,
        )
        .await?
}

async fn app_main() -> anyhow::Result<()> {
    info!("Hello, world!");
    info_span!("Test Span").in_scope(|| {
        info!("Info Event!");
        warn!("Warn Event!")
    });
    info!(a = 123, b = "String", "start do");
    for _ in 0..10 {
        info!("do something");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    info!("App End!");
    Ok(())
}
```

更多用例请看 examples

[//]: # ()

[//]: # (## Todo)

[//]: # ()

[//]: # (- Offline、Import、Export)

[//]: # (- More Filter)

[//]: # (- Timeline)

[//]: # (- Setting)

[//]: # (- App、Node Manager)

[//]: # (- Notify)

[//]: # (- ...)



## License

MIT License ([LICENSE-MIT](https://github.com/ycysdf/tracing-live/blob/main/LICENSE-MIT))