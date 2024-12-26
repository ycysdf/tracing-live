use crate::dyn_query::ApplyFilterOp;
use crate::grpc_service::{
    RunningSpan, SpanFullInfo, SpanInfo, TracingFields, TracingServiceImpl, FIELD_DATA_SPAN_T_ID,
};
use crate::tracing_service::{
    BigInt, TracingLevel, TracingRecordDto, TracingRecordFieldFilter, TracingRecordFilter,
    TracingRecordScene, TracingTreeRecordDto,
};
use chrono::{DateTime, Local, Utc};
use entity::prelude::TracingRecord;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use smallvec::smallvec;
use smol_str::{SmolStr, ToSmolStr};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing_lv_proto::{field_value, FieldValue, PosInfo};
use utoipa::ToSchema;
use uuid::Uuid;

pub type SpanId = Uuid;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, ToSchema)]
pub enum TracingKind {
    SpanCreate,
    SpanEnter,
    SpanLeave,
    SpanClose,
    SpanRecord,
    Event,
    RepEvent,
    AppStart,
    AppStop,
}

impl FromStr for TracingKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "SPAN_CREATE" => TracingKind::SpanCreate,
            "SPAN_ENTER" => TracingKind::SpanEnter,
            "SPAN_LEAVE" => TracingKind::SpanLeave,
            "SPAN_CLOSE" => TracingKind::SpanClose,
            "SPAN_RECORD" => TracingKind::SpanRecord,
            "EVENT" => TracingKind::Event,
            "REP_EVENT" => TracingKind::RepEvent,
            "APP_START" => TracingKind::AppStart,
            "APP_STOP" => TracingKind::AppStop,
            _ => return Err(()),
        })
    }
}

impl TracingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TracingKind::SpanCreate => "SPAN_CREATE",
            TracingKind::SpanEnter => "SPAN_ENTER",
            TracingKind::SpanLeave => "SPAN_LEAVE",
            TracingKind::SpanClose => "SPAN_CLOSE",
            TracingKind::SpanRecord => "SPAN_RECORD",
            TracingKind::Event => "EVENT",
            TracingKind::RepEvent => "REP_EVENT",
            TracingKind::AppStart => "APP_START",
            TracingKind::AppStop => "APP_STOP",
        }
    }
}

#[derive(Clone, Debug)]
pub enum TracingRecordVariant {
    SpanCreate {
        name: SmolStr,
        parent_span_info: Option<Arc<SpanInfo>>,
        info: SpanFullInfo,
    },
    SpanEnter {
        info: SpanFullInfo,
    },
    SpanLeave {
        info: SpanFullInfo,
    },
    SpanClose {
        info: SpanFullInfo,
    },
    SpanRecord {
        info: SpanFullInfo,
    },
    Event {
        record_time: DateTime<Utc>,
        app_info: Arc<AppRunInfo>,
        message: SmolStr,
        span_info: Option<Arc<SpanInfo>>,
        running_span: Option<RunningSpan>,
        fields: TracingFields,
        target: SmolStr,
        module_path: Option<SmolStr>,
        file_line: Option<SmolStr>,
        level: tracing_lv_proto::Level,
        is_repeated_event: bool,
    },
    AppStart {
        record_time: DateTime<Utc>,
        app_info: Arc<AppRunInfo>,
        name: SmolStr,
        fields: TracingFields,
    },
    AppStop {
        record_time: DateTime<Utc>,
        app_info: Arc<AppRunInfo>,
        name: SmolStr,
        exception_end: bool,
    },
}

impl TracingRecordVariant {
    #[inline(always)]
    pub fn get_dto(&self, record_id: BigInt) -> TracingRecordDto {
        TracingRecordDto {
            id: record_id,
            app_id: self.app_info().id,
            app_version: self.app_info().version.clone(),
            app_run_id: self.app_info().run_id,
            node_id: self.app_info().node_id.clone(),
            name: self.name().clone(),
            kind: self.kind(),
            level: self.level(),
            span_id: self.span_id(),
            fields: Arc::new(
                self.fields()
                    .cloned()
                    .map(|n| n.into_json_map())
                    .unwrap_or_default(),
            ),
            span_id_is_stable: self
                .span_full_info()
                .map(|n| n.fields.stable_span_id().is_some()),
            record_time: self.record_time().fixed_offset(),
            target: self.target().cloned(),
            module_path: self.module_path().cloned(),
            position_info: self.file_line().cloned(),
            creation_time: Utc::now().fixed_offset(),
            parent_id: self.parent_id(),
            span_t_id: self.span_t_id().map(|n|n.to_smolstr()),
            parent_span_t_id: self.parent_span_t_id().map(|n|n.to_smolstr()),
            repeated_count: None,
        }
    }

    #[inline(always)]
    pub fn into_dto(mut self, record_id: BigInt) -> TracingRecordDto {
        TracingRecordDto {
            id: record_id,
            app_id: self.app_info().id,
            app_version: self.app_info().version.clone(),
            app_run_id: self.app_info().run_id,
            node_id: self.app_info().node_id.clone(),
            name: self.name().clone(),
            kind: self.kind(),
            level: self.level(),
            span_id: self.span_id(),
            fields: Arc::new(
                self.fields_mut()
                    .map(|n| core::mem::take(n).into_json_map())
                    .unwrap_or_default(),
            ),
            span_id_is_stable: self
                .span_full_info()
                .map(|n| n.fields.stable_span_id().is_some()),
            record_time: self.record_time().fixed_offset(),
            target: self.target().cloned(),
            module_path: self.module_path().cloned(),
            position_info: self.file_line().cloned(),
            creation_time: Utc::now().fixed_offset(),
            parent_id: self.parent_id(),
            span_t_id: self.span_t_id().map(|n|n.to_smolstr()),
            parent_span_t_id: self.parent_span_t_id().map(|n|n.to_smolstr()),
            repeated_count: None,
        }
    }

    #[inline]
    pub fn filter(&self, filter: &TracingRecordFilter) -> bool {
        let app_info = self.app_info();
        if let Some(scene) = &filter.scene {
            match scene {
                TracingRecordScene::Tree => {
                    if [TracingKind::SpanEnter, TracingKind::SpanLeave].contains(&self.kind()) {
                        return false;
                    }
                }
                TracingRecordScene::SpanField => {
                    if ![TracingKind::SpanRecord].contains(&self.kind()) {
                        return false;
                    }
                }
                TracingRecordScene::SpanEnter => {
                    if ![TracingKind::SpanEnter, TracingKind::SpanLeave].contains(&self.kind()) {
                        return false;
                    }
                }
            }
        }
        if let Some(app_builds) = &filter.app_build_ids {
            if !app_builds.iter().any(|(app_id, version)| {
                app_info.id == *app_id
                    && version
                        .as_ref()
                        .map(|n| &app_info.version == n)
                        .unwrap_or(true)
            }) {
                return false;
            }
        }
        if let Some(node_ids) = &filter.node_ids {
            if !node_ids.iter().any(|n| n.as_str() == &app_info.node_id) {
                return false;
            }
        }
        if let Some(app_run_ids) = &filter.app_run_ids {
            if !app_run_ids.contains(&app_info.run_id) {
                return false;
            }
        }
        if let Some(start_time) = &filter.start_time {
            if &self.record_time() < start_time {
                return false;
            }
        }
        if let Some(end_time) = &filter.end_time {
            if &self.record_time() > end_time {
                return false;
            }
        }
        if let Some(parent_id) = &filter.parent_id {
            if parent_id.as_u128() == 0 {
                if self.parent_id().is_some() {
                    return false;
                }
            } else {
                if self.parent_id().as_ref() != Some(parent_id) {
                    return false;
                }
            }
        }
        if let Some(span_t_id) = &filter.parent_span_t_id {
            if let Some(parent_id) = self.parent_span_t_id().as_ref() {
                if *parent_id != *span_t_id {
                    return false;
                }
            } else {
                return false;
            }
        }
        if let Some(kinds) = &filter.kinds {
            let kind = self.kind().as_str();
            if !kinds.is_empty() && !kinds.iter().any(|n| n.as_str() == kind) {
                return false;
            }
        }
        if let Some(span_ids) = &filter.span_ids {
            if !span_ids.is_empty() {
                if let Some(span_id) = self.span_id() {
                    if !span_ids.contains(&span_id) {
                        return false;
                    }
                }
            }
        }
        if let Some(targets) = &filter.targets {
            if !targets.is_empty() {
                if let Some(target) = self.target() {
                    for (op, value) in targets {
                        if !target.as_str().apply_filter_op(value.as_str(), op.clone()) {
                            return false;
                        }
                    }
                }
            }
        }
        if let Some(levels) = &filter.levels {
            if !levels.is_empty() {
                if let Some(level) = self.level() {
                    if !levels.contains(&level) {
                        return false;
                    }
                }
            }
        }
        if let Some((op, name)) = &filter.name {
            if !self
                .name()
                .as_str()
                .apply_filter_op(name.as_str(), op.clone())
            {
                return false;
            }
        }
        for TracingRecordFieldFilter { name, op, value } in filter
            .fields
            .as_ref()
            .unwrap_or(&smallvec![])
            .iter()
            .filter(|n| n.value.is_some())
        {
            match name.as_str() {
                FIELD_DATA_SPAN_T_ID => {
                    let Ok(value) = value.as_ref().unwrap().parse::<u64>() else {
                        return true;
                    };
                    if let Some(span_t_id) = self.span_t_id() {
                        if !span_t_id.apply_filter_op(value, *op) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                &_ => {}
            }
        }
        // if let Some(parent_id) = &filter.parent_id {
        //     if !app_run_ids.contains(&self.app_info().id) {
        //         return false;
        //     }
        // }
        true
    }

    #[inline(always)]
    pub fn fields(&self) -> Option<&TracingFields> {
        match self {
            TracingRecordVariant::Event { fields, .. } => Some(fields),
            _ => self.span_full_info().map(|n| &n.fields),
        }
    }
    #[inline(always)]
    pub fn fields_mut(&mut self) -> Option<&mut TracingFields> {
        match self {
            TracingRecordVariant::Event { fields, .. } => Some(fields),
            TracingRecordVariant::AppStart { fields, .. } => Some(fields),
            _ => self.span_full_info_mut().map(|n| &mut n.fields),
        }
    }

    #[inline(always)]
    pub fn scoped_json_fields<U>(
        &mut self,
        f: impl FnOnce(&Self, &Option<serde_json::Value>) -> U,
    ) -> U {
        let option = self.fields_mut().take().map(core::mem::take);
        let (r, fields) = Self::scoped_json_fields_by(option, |fields| f(self, fields));
        if let Some(fields1) = self.fields_mut() {
            core::mem::swap(fields1, &mut fields.unwrap());
        }
        r
    }

    #[inline(always)]
    pub fn scoped_json_fields_by<U>(
        fields: Option<TracingFields>,
        f: impl FnOnce(&Option<serde_json::Value>) -> U,
    ) -> (U, Option<TracingFields>) {
        match fields {
            None => (f(&None), None),
            Some(mut fields) => {
                let fields = core::mem::take(&mut fields);
                let json_fields = fields.into_json_map();
                let json_fields = Some(serde_json::value::Value::Object(json_fields));
                let r = f(&json_fields);
                let serde_json::Value::Object(json_fields) = json_fields.unwrap() else {
                    unreachable!()
                };
                let fields1 = TracingFields::new(
                    json_fields.into_iter().map(|n| (n.0, n.1.into())).collect(),
                );
                (r, Some(fields1))
            }
        }
    }

    #[inline(always)]
    pub fn kind(&self) -> TracingKind {
        match self {
            TracingRecordVariant::SpanCreate { .. } => TracingKind::SpanCreate,
            TracingRecordVariant::SpanEnter { .. } => TracingKind::SpanEnter,
            TracingRecordVariant::SpanLeave { .. } => TracingKind::SpanLeave,
            TracingRecordVariant::SpanClose { .. } => TracingKind::SpanClose,
            TracingRecordVariant::SpanRecord { .. } => TracingKind::SpanRecord,
            TracingRecordVariant::Event {
                is_repeated_event, ..
            } => {
                if *is_repeated_event {
                    TracingKind::RepEvent
                } else {
                    TracingKind::Event
                }
            }
            TracingRecordVariant::AppStart { .. } => TracingKind::AppStart,
            TracingRecordVariant::AppStop { .. } => TracingKind::AppStop,
        }
    }
    #[inline(always)]
    pub fn record_time(&self) -> DateTime<Utc> {
        match self {
            TracingRecordVariant::Event { record_time, .. } => record_time.clone(),
            TracingRecordVariant::AppStart { record_time, .. } => record_time.clone(),
            TracingRecordVariant::AppStop { record_time, .. } => record_time.clone(),
            n => n.span_full_info().unwrap().record_time.clone(),
        }
    }

    #[inline(always)]
    pub fn name(&self) -> &SmolStr {
        match &self {
            TracingRecordVariant::Event { message, .. } => message,
            TracingRecordVariant::AppStart { name, .. } => name,
            TracingRecordVariant::AppStop { name, .. } => name,
            _ => &self.span_full_info().unwrap().cache_id.name,
        }
    }

    #[inline(always)]
    pub fn span_info(&self) -> Option<&SpanInfo> {
        Some(match self {
            TracingRecordVariant::Event { span_info, .. } => span_info.as_ref()?,
            n => n.span_full_info()?.span_info.as_ref(),
        })
    }
    #[inline(always)]
    pub fn span_full_info(&self) -> Option<&SpanFullInfo> {
        Some(match self {
            TracingRecordVariant::SpanCreate { info, .. } => &info,
            TracingRecordVariant::SpanEnter { info, .. } => &info,
            TracingRecordVariant::SpanLeave { info, .. } => &info,
            TracingRecordVariant::SpanClose { info, .. } => &info,
            TracingRecordVariant::SpanRecord { info, .. } => &info,
            _ => return None,
        })
    }
    #[inline(always)]
    pub fn span_full_info_mut(&mut self) -> Option<&mut SpanFullInfo> {
        Some(match self {
            TracingRecordVariant::SpanCreate { info, .. } => info,
            TracingRecordVariant::SpanEnter { info, .. } => info,
            TracingRecordVariant::SpanLeave { info, .. } => info,
            TracingRecordVariant::SpanClose { info, .. } => info,
            TracingRecordVariant::SpanRecord { info, .. } => info,
            _ => return None,
        })
    }
    #[inline(always)]
    pub fn app_info(&self) -> &Arc<AppRunInfo> {
        match self {
            TracingRecordVariant::SpanCreate { info, .. } => &info.app_info,
            TracingRecordVariant::SpanEnter { info, .. } => &info.app_info,
            TracingRecordVariant::SpanLeave { info, .. } => &info.app_info,
            TracingRecordVariant::SpanClose { info, .. } => &info.app_info,
            TracingRecordVariant::SpanRecord { info, .. } => &info.app_info,
            TracingRecordVariant::Event { app_info, .. } => app_info,
            TracingRecordVariant::AppStart { app_info, .. } => app_info,
            TracingRecordVariant::AppStop { app_info, .. } => app_info,
        }
    }
    #[inline(always)]
    pub fn span_t_id(&self) -> Option<u64> {
        Some(match self {
            TracingRecordVariant::Event { span_info, .. } => span_info.as_ref()?.t_id,
            n => n.span_full_info()?.t_id,
        })
    }

    #[inline(always)]
    pub fn parent_span_t_id(&self) -> Option<u64> {
        Some(match self {
            TracingRecordVariant::Event { span_info, .. } => span_info.as_ref()?.t_id,
            _ => self.span_full_info()?.running_span.parent_span_t_id?,
        })
    }
    #[inline(always)]
    pub fn span_id(&self) -> Option<Uuid> {
        self.span_full_info().map(|n| n.running_span.id.clone())
    }
    #[inline(always)]
    pub fn target(&self) -> Option<&SmolStr> {
        Some(match self {
            TracingRecordVariant::Event { target, .. } => target,
            n => return n.span_full_info()?.running_span.target.as_ref(),
        })
    }
    #[inline(always)]
    pub fn file_line(&self) -> Option<&SmolStr> {
        match self {
            TracingRecordVariant::Event { file_line, .. } => file_line.as_ref(),
            n => Some(&n.span_full_info()?.cache_id.file_line),
        }
    }
    #[inline(always)]
    pub fn module_path(&self) -> Option<&SmolStr> {
        match self {
            TracingRecordVariant::Event { module_path, .. } => module_path.as_ref(),
            n => Some(&n.span_full_info()?.module_path),
        }
    }
    #[inline(always)]
    pub fn level(&self) -> Option<TracingLevel> {
        Some(match self {
            TracingRecordVariant::Event { level, .. } => level.clone().into(),
            n => return n.span_full_info()?.running_span.level?.clone().into(),
        })
    }

    // SPAN SPAN_XXX
    #[inline(always)]
    pub fn parent_id(&self) -> Option<SpanId> {
        Some(match self {
            TracingRecordVariant::Event { running_span, .. } => running_span.as_ref()?.id,
            n => return n.span_full_info()?.running_span.parent.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct AppRunInfo {
    pub id: Uuid,
    pub version: SmolStr,
    pub run_id: Uuid,
    pub node_id: SmolStr,
}

#[derive(Hash, Clone, Eq, PartialEq, Debug)]
pub struct SpanCacheId {
    pub app_id: Uuid,
    pub app_version: SmolStr,
    pub name: SmolStr,
    pub file_line: SmolStr,
}

impl SpanCacheId {
    pub fn new(app_info: &AppRunInfo, span_info: tracing_lv_proto::SpanInfo) -> Self {
        Self {
            app_id: app_info.id,
            app_version: app_info.version.clone(),
            name: span_info.name.into(),
            file_line: span_info.file_line.into(),
        }
    }
}
