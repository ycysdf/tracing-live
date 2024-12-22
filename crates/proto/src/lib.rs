use serde_json::Value;
mod flags;
pub use flags::*;
mod generated_tracing;

pub use generated_tracing::*;

// tonic::include_proto!("tracing");

impl From<f32> for field_value::Variant {
    fn from(value: f32) -> Self {
        field_value::Variant::F64(value.into())
    }
}
impl From<f64> for field_value::Variant {
    fn from(value: f64) -> Self {
        field_value::Variant::F64(value.into())
    }
}

impl From<u32> for field_value::Variant {
    fn from(value: u32) -> Self {
        field_value::Variant::U64(value.into())
    }
}

impl From<u64> for field_value::Variant {
    fn from(value: u64) -> Self {
        field_value::Variant::U64(value.into())
    }
}

impl From<bool> for field_value::Variant {
    fn from(value: bool) -> Self {
        field_value::Variant::Bool(value)
    }
}

impl From<String> for field_value::Variant {
    fn from(value: String) -> Self {
        field_value::Variant::String(value)
    }
}

impl From<&'static str> for field_value::Variant {
    fn from(value: &'static str) -> Self {
        field_value::Variant::String(value.into())
    }
}

impl From<FieldValue> for serde_json::Value {
    fn from(value: FieldValue) -> Self {
        value.variant.unwrap().into()
    }
}

impl From<field_value::Variant> for serde_json::Value {
    fn from(value: field_value::Variant) -> Self {
        match value {
            field_value::Variant::F64(n) => {
                serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap())
            }
            field_value::Variant::I64(n) => serde_json::Value::Number(n.into()),
            field_value::Variant::U64(n) => serde_json::Value::Number(n.into()),
            field_value::Variant::Bool(n) => serde_json::Value::Bool(n),
            field_value::Variant::String(n) => serde_json::Value::String(n),
            field_value::Variant::Null(_) => serde_json::Value::Null,
        }
    }
}

impl From<serde_json::Value> for field_value::Variant {
    fn from(value: Value) -> Self {
        match value {
            serde_json::Value::Number(n) if n.is_f64() => {
                field_value::Variant::F64(n.as_f64().unwrap())
            }
            serde_json::Value::Number(n) if n.is_u64() => {
                field_value::Variant::U64(n.as_u64().unwrap())
            }
            serde_json::Value::Number(n) if n.is_i64() => {
                field_value::Variant::I64(n.as_i64().unwrap())
            }
            serde_json::Value::Bool(n) => field_value::Variant::Bool(n),
            serde_json::Value::String(n) => field_value::Variant::String(n),
            serde_json::Value::Null => field_value::Variant::Null(true),
            _ => field_value::Variant::Null(false),
        }
    }
}

impl From<serde_json::Value> for FieldValue {
    fn from(value: Value) -> Self {
        let variant = value.into();
        FieldValue {
            variant: Some(variant),
        }
    }
}
