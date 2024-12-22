use chrono::Utc;
use hashbrown::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;
use uuid::Uuid;

pub struct GlobalRunningApp {
    delta_date_nanos: i64,
}

pub static GLOBAL_DATA: LazyLock<GlobalData> = LazyLock::new(GlobalData::new);

pub struct GlobalData {
    running_apps: Arc<RwLock<HashMap<Uuid, GlobalRunningApp>>>,
}

impl GlobalData {
    pub fn new() -> Self {
        Self {
            running_apps: Arc::new(Default::default()),
        }
    }
    pub fn add_running_app(&self, run_id: Uuid, delta_date_nanos: i64) {
        self.running_apps
            .write()
            .unwrap()
            .insert(run_id, GlobalRunningApp { delta_date_nanos });
    }
    pub fn get_running_app_delta_date_nanos(&self, run_id: Uuid) -> Option<i64> {
        self.running_apps
            .read()
            .unwrap()
            .get(&run_id)
            .map(|app| app.delta_date_nanos)
    }
    pub fn remove_running_app(&self, run_id: Uuid) -> Option<GlobalRunningApp> {
        self.running_apps.write().unwrap().remove(&run_id)
    }

    pub fn get_node_now_timestamp_nanos(&self, run_id: Uuid) -> Option<i64> {
        let timestamp_nanos = Utc::now().timestamp_nanos_opt().unwrap();
        GLOBAL_DATA
            .get_running_app_delta_date_nanos(run_id)
            .map(|delta_date| timestamp_nanos - delta_date)
    }
}
