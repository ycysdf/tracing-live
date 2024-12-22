use tracing::{info, info_span, warn};
use tracing_lv::client::{TLAppInfo, TLSubscriberExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .tracing_lv_init(
            "http://localhost:8080",
            TLAppInfo::new(
                uuid::uuid!("f7570e13-d331-40f7-8d04-e87530308c05"), // App Identify
                "Simple App",                                        // App Name
                "1.0", // App Version. Code Changed. this value should change
            )
            .node_id("TestNode") // Node Identify
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
