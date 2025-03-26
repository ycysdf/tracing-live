use crate::dyn_query::ApplyFilterOp;
use crate::rpc_service::SpanFullInfo;
use crate::running_app::CreatedSpan;
use crate::tracing_service::{
    TracingLevel, TracingRecordDto, TracingRecordFieldFilter, TracingRecordFilter,
    TracingRecordScene, TracingTreeRecordDto,
};
use chrono::{DateTime, Local, Utc};
use derive_more::{Deref, DerefMut, Display, From, FromStr};
use entity::prelude::TracingRecord;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use smallvec::smallvec;
use smol_str::{SmolStr, ToSmolStr, format_smolstr};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tracing_lv_core::{SpanRawInfo, TracingFields, VxMetadata};
use utoipa::ToSchema;
use uuid::Uuid;

pub type SpanId = Uuid;

#[derive(Serialize, Deserialize, Display, FromStr, PartialEq, Eq, Clone, Debug, ToSchema)]
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
    DataUpdate,
    RelatedEvent,
}

impl TracingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SpanCreate => "SpanCreate",
            Self::SpanEnter => "SpanEnter",
            Self::SpanLeave => "SpanLeave",
            Self::SpanClose => "SpanClose",
            Self::SpanRecord => "SpanRecord",
            Self::Event => "Event",
            Self::RepEvent => "RepEvent",
            Self::AppStart => "AppStart",
            Self::AppStop => "AppStop",
            Self::DataUpdate => "DataUpdate",
            Self::RelatedEvent => "RelatedEvent",
        }
    }
}

pub trait SmolStrExt {
    fn into_smol_str(self) -> SmolStr;
}

impl SmolStrExt for Cow<'static, str> {
    fn into_smol_str(self) -> SmolStr {
        match self {
            Cow::Borrowed(n) => SmolStr::new_static(n),
            Cow::Owned(n) => n.into(),
        }
    }
}

pub type TraceId = u64;

#[derive(Deref, DerefMut, Debug, Clone)]
pub struct TracingSpanInfo {
    pub trace_id: TraceId,
    pub parent_trace_id: Option<TraceId>,
    pub parent_id: Option<SpanId>,
    pub id: SpanId,
    #[deref_mut]
    #[deref]
    pub tl_base: Arc<TLSpanInfo>,
}

#[derive(Debug)]
pub struct TLSpanInfo {
    pub name: SmolStr,
    pub target: SmolStr,
    pub level: TracingLevel,
    pub module_path: SmolStr,
    pub file: Option<SmolStr>,
    pub line: Option<u32>,
}

impl From<SpanRawInfo> for TLSpanInfo {
    fn from(value: SpanRawInfo) -> Self {
        Self {
            name: value.metadata.name.into_smol_str(),
            target: value.metadata.target.into_smol_str(),
            level: value.metadata.level.as_ref().try_into().unwrap(),
            module_path: value
                .metadata
                .module_path
                .map(|n| n.into_smol_str())
                .unwrap_or(SmolStr::new_static("<UNKNOWN>")),
            file: value.metadata.file.map(|n| n.into_smol_str()),
            line: value.metadata.line,
        }
    }
}
impl TLSpanInfo {
    pub fn file_line(&self) -> SmolStr {
        format_smolstr!(
            "{}:{}",
            self.file.as_ref().map(|n| n.as_str()).unwrap_or("UNKNOWN"),
            self.line.as_ref().unwrap_or(&0)
        )
    }
}

#[derive(Deref, DerefMut, Debug, Clone)]
pub struct SpanRecordItem {
    pub record_index: u64,
    #[deref_mut]
    #[deref]
    pub span: TracingSpanInfo,
    pub fields: TracingFields,
    pub record_date: DateTime<Utc>,
    pub span_parent: Option<TracingSpanInfo>,
}

#[derive(Clone, Debug)]
pub struct EventRecordItem {
    pub fields: TracingFields,
    pub record_date: DateTime<Utc>,
    pub message: SmolStr,
    pub span: Option<TracingSpanInfo>,
    pub target: SmolStr,
    pub level: TracingLevel,
    pub module_path: SmolStr,
    pub file: Option<SmolStr>,
    pub line: Option<u32>,
    pub record_index: u64,
    pub is_related_event: bool,
    pub is_repeated_event: bool,
}

impl EventRecordItem {
    pub fn file_line(&self) -> SmolStr {
        format_smolstr!(
            "{}:{}",
            self.file.as_ref().map(|n| n.as_str()).unwrap_or("UNKNOWN"),
            self.line.as_ref().unwrap_or(&0)
        )
    }
}

#[derive(Clone, Debug)]
pub enum TracingRecordVariant {
    SpanCreate {
        span_item: SpanRecordItem,
    },
    SpanEnter {
        span: TracingSpanInfo,
        record_date: DateTime<Utc>,
        record_index: u64,
    },
    SpanLeave {
        span: TracingSpanInfo,
        record_date: DateTime<Utc>,
        record_index: u64,
    },
    SpanClose {
        span: TracingSpanInfo,
        record_date: DateTime<Utc>,
        record_index: u64,
    },
    SpanRecord {
        span: TracingSpanInfo,
        fields: TracingFields,
        record_date: DateTime<Utc>,
        record_index: u64,
    },
    Event {
        event_item: EventRecordItem,
    },
    AppStart {
        record_date: DateTime<Utc>,
        app_info: Arc<AppRunInfo>,
        name: SmolStr,
        fields: TracingFields,
        reconnect: bool,
        created_spans: hashbrown::HashMap<u64, CreatedSpan>,
    },
    AppStop {
        record_date: DateTime<Utc>,
        app_info: Arc<AppRunInfo>,
        name: SmolStr,
        exception_end: bool,
    },
}

impl TracingRecordVariant {
    // #[inline(always)]
    // pub fn get_dto(&self, record_id: u64) -> TracingRecordDto {
    //     TracingRecordDto {
    //         id: record_id,
    //         record_index: 0,
    //         app_id: self.app_info().id,
    //         app_version: self.app_info().version.clone(),
    //         app_run_id: self.app_info().run_id,
    //         node_id: self.app_info().node_id.clone(),
    //         name: self.name().clone(),
    //         kind: self.kind(),
    //         level: self.level(),
    //         span_id: self.span_id(),
    //         fields: Arc::new(
    //             self.fields()
    //                 .cloned()
    //                 .map(|n| n.into_json_map())
    //                 .unwrap_or_default(),
    //         ),
    //         span_id_is_stable: self
    //             .span_full_info()
    //             .map(|n| n.fields.stable_span_id().is_some()),
    //         record_time: self.record_time().fixed_offset(),
    //         target: self.target().cloned(),
    //         module_path: self.module_path().cloned(),
    //         position_info: self.file_line().cloned(),
    //         creation_time: Utc::now().fixed_offset(),
    //         parent_id: self.parent_id(),
    //         span_t_id: self.span_t_id().map(|n| n.to_smolstr()),
    //         parent_span_t_id: self.parent_span_t_id().map(|n| n.to_smolstr()),
    //         repeated_count: None,
    //     }
    // }

    #[inline]
    pub fn filter(&self, filter: &TracingRecordFilter,app_info: &AppRunInfo) -> bool {
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
            let me_parent_id = if let TracingRecordVariant::Event {
                event_item:
                    EventRecordItem {
                        is_related_event,
                        span: Some(span_info),
                        ..
                    },
                ..
            } = &self
            {
                if *is_related_event {
                    span_info.parent_id
                } else {
                    self.parent_id()
                }
            } else {
                self.parent_id()
            };
            if parent_id.as_u128() == 0 {
                if me_parent_id.is_some() {
                    return false;
                }
            } else {
                if me_parent_id.as_ref() != Some(parent_id) {
                    return false;
                }
            }
        }
        if let Some(parent_span_t_ids) = &filter.parent_span_t_ids {
            let parent_span_t_id = if let TracingRecordVariant::Event {
                event_item:
                    EventRecordItem {
                        is_related_event,
                        span: Some(span_info),
                        ..
                    },
                ..
            } = &self
            {
                if *is_related_event {
                    span_info.parent_trace_id
                } else {
                    self.parent_span_trace_id()
                }
            } else {
                self.parent_span_trace_id()
            };
            if let Some(parent_id) = parent_span_t_id {
                if !parent_span_t_ids.is_empty() && !parent_span_t_ids.contains(&parent_id) {
                    return false;
                }
            } else {
                if !parent_span_t_ids.iter().all(|n| *n == 0) {
                    return false;
                }
            }
        }
        if let Some(kinds) = &filter.kinds {
            let kind = self.kind();
            if !kinds.is_empty() && !kinds.iter().any(|n| n == &kind) {
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
            TracingRecordVariant::Event { event_item, .. } => Some(&event_item.fields),
            TracingRecordVariant::AppStart { fields, .. } => Some(fields),
            TracingRecordVariant::SpanCreate { span_item } => Some(&span_item.fields),
            TracingRecordVariant::SpanRecord { fields, .. } => Some(fields),
            _ => None,
        }
    }
    #[inline(always)]
    pub fn fields_mut(&mut self) -> Option<&mut TracingFields> {
        match self {
            TracingRecordVariant::Event { event_item, .. } => Some(&mut event_item.fields),
            TracingRecordVariant::AppStart { fields, .. } => Some(fields),
            TracingRecordVariant::SpanCreate { span_item } => Some(&mut span_item.fields),
            TracingRecordVariant::SpanRecord { fields, .. } => Some(fields),
            _ => None,
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
                    json_fields.into_iter().map(|n| (n.0.into(), n.1.try_into().unwrap())).collect(),
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
                event_item:
                    EventRecordItem {
                        is_repeated_event,
                        is_related_event,
                        ..
                    },
            } => match (*is_related_event, *is_repeated_event) {
                (true, _) => TracingKind::RelatedEvent,
                (false, true) => TracingKind::RepEvent,
                (false, false) => TracingKind::Event,
            },
            TracingRecordVariant::AppStart { .. } => TracingKind::AppStart,
            TracingRecordVariant::AppStop { .. } => TracingKind::AppStop,
        }
    }
    #[inline(always)]
    pub fn record_time(&self) -> DateTime<Utc> {
        match self {
            TracingRecordVariant::Event { event_item, .. } => event_item.record_date.clone(),
            TracingRecordVariant::AppStart { record_date, .. } => record_date.clone(),
            TracingRecordVariant::AppStop { record_date, .. } => record_date.clone(),
            TracingRecordVariant::SpanCreate { span_item, .. } => span_item.record_date.clone(),
            TracingRecordVariant::SpanEnter { record_date, .. } => record_date.clone(),
            TracingRecordVariant::SpanLeave { record_date, .. } => record_date.clone(),
            TracingRecordVariant::SpanClose { record_date, .. } => record_date.clone(),
            TracingRecordVariant::SpanRecord { record_date, .. } => record_date.clone(),
        }
    }

    #[inline(always)]
    pub fn name(&self) -> &SmolStr {
        match &self {
            TracingRecordVariant::Event { event_item, .. } => &event_item.message,
            TracingRecordVariant::AppStart { name, .. } => name,
            TracingRecordVariant::AppStop { name, .. } => name,
            _ => &self.span_info().unwrap().name,
        }
    }

    #[inline(always)]
    pub fn span_info(&self) -> Option<&TracingSpanInfo> {
        Some(match self {
            TracingRecordVariant::SpanCreate { span_item, .. } => &span_item,
            TracingRecordVariant::SpanEnter { span, .. } => &span,
            TracingRecordVariant::SpanLeave { span, .. } => &span,
            TracingRecordVariant::SpanClose { span, .. } => &span,
            TracingRecordVariant::SpanRecord { span, .. } => &span,
            TracingRecordVariant::Event { event_item, .. } => event_item.span.as_ref()?,
            _ => return None,
        })
    }
    #[inline(always)]
    pub fn span_info_mut(&mut self) -> Option<&mut TracingSpanInfo> {
        Some(match self {
            TracingRecordVariant::SpanCreate { span_item, .. } => span_item,
            TracingRecordVariant::SpanEnter { span, .. } => span,
            TracingRecordVariant::SpanLeave { span, .. } => span,
            TracingRecordVariant::SpanClose { span, .. } => span,
            TracingRecordVariant::SpanRecord { span, .. } => span,
            TracingRecordVariant::Event { event_item, .. } => event_item.span.as_mut()?,
            _ => return None,
        })
    }
    // #[inline(always)]
    // pub fn app_info(&self) -> &Arc<AppRunInfo> {
    //     match self {
    //         TracingRecordVariant::SpanCreate { info, .. } => &info.app_info,
    //         TracingRecordVariant::SpanEnter { info, .. } => &info.app_info,
    //         TracingRecordVariant::SpanLeave { info, .. } => &info.app_info,
    //         TracingRecordVariant::SpanClose { info, .. } => &info.app_info,
    //         TracingRecordVariant::SpanRecord { info, .. } => &info.app_info,
    //         TracingRecordVariant::Event { app_info, .. } => app_info,
    //         TracingRecordVariant::AppStart { app_info, .. } => app_info,
    //         TracingRecordVariant::AppStop { app_info, .. } => app_info,
    //     }
    // }
    #[inline(always)]
    pub fn span_t_id(&self) -> Option<u64> {
        Some(self.span_info()?.trace_id)
    }

    #[inline(always)]
    pub fn parent_span_trace_id(&self) -> Option<u64> {
        Some(match self {
            TracingRecordVariant::Event { event_item, .. } => event_item.span.as_ref()?.trace_id,
            _ => self.span_info()?.trace_id,
        })
    }
    #[inline(always)]
    pub fn span_id(&self) -> Option<Uuid> {
        self.span_info().map(|n| n.id.clone())
    }
    #[inline(always)]
    pub fn target(&self) -> Option<&SmolStr> {
        Some(match self {
            TracingRecordVariant::Event { event_item, .. } => &event_item.target,
            n => &n.span_info()?.target,
        })
    }
    #[inline(always)]
    pub fn file_line(&self) -> Option<SmolStr> {
        Some(match self {
            TracingRecordVariant::Event { event_item, .. } => event_item.file_line(),
            n => n.span_info()?.file_line(),
        })
    }
    #[inline(always)]
    pub fn module_path(&self) -> Option<&SmolStr> {
        Some(match self {
            TracingRecordVariant::Event { event_item, .. } => &event_item.module_path,
            n => &n.span_info()?.module_path,
        })
    }
    #[inline(always)]
    pub fn level(&self) -> Option<TracingLevel> {
        Some(match self {
            TracingRecordVariant::Event { event_item, .. } => event_item.level.clone(),
            n => return Some(n.span_info()?.level.clone()),
        })
    }

    // SPAN SPAN_XXX
    #[inline(always)]
    pub fn parent_id(&self) -> Option<SpanId> {
        Some(match self {
            TracingRecordVariant::Event { event_item, .. } => event_item.span.as_ref()?.id,
            n => return n.span_info()?.parent_id.clone(),
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
