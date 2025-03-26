use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sea_orm::prelude::{Json, StringLen};
use sea_orm::sea_query::{
    Alias, BinOper, ColumnRef, ConditionExpression, IntoCondition, SimpleExpr, UnOper,
};
use sea_orm::{ColumnType, Condition, DynIden, Value};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, ToSchema, Serialize, Deserialize)]
pub enum TLDbValue {
    Bool(Option<bool>),
    TinyInt(Option<i8>),
    SmallInt(Option<i16>),
    Int(Option<i32>),
    BigInt(Option<i64>),
    TinyUnsigned(Option<u8>),
    SmallUnsigned(Option<u16>),
    Unsigned(Option<u32>),
    BigUnsigned(Option<u64>),
    Float(Option<f32>),
    Double(Option<f64>),
    String(Option<Box<String>>),
    Char(Option<char>),

    #[allow(clippy::box_collection)]
    Bytes(Option<Box<Vec<u8>>>),

    Json(Option<Box<Json>>),

    ChronoDate(Option<Box<NaiveDate>>),

    ChronoTime(Option<Box<NaiveTime>>),

    ChronoDateTime(Option<Box<NaiveDateTime>>),

    ChronoDateTimeUtc(Option<Box<DateTime<Utc>>>),

    ChronoDateTimeLocal(Option<Box<DateTime<Local>>>),

    ChronoDateTimeWithTimeZone(Option<Box<DateTime<FixedOffset>>>),

    Uuid(Option<Box<Uuid>>),
    // Array(ArrayType, Option<Box<Vec<TLValue>>>),
}

impl From<TLDbValue> for Value {
    fn from(value: TLDbValue) -> Self {
        match value {
            TLDbValue::Bool(n) => Value::Bool(n),
            TLDbValue::TinyInt(n) => Value::TinyInt(n),
            TLDbValue::SmallInt(n) => Value::SmallInt(n),
            TLDbValue::Int(n) => Value::Int(n),
            TLDbValue::BigInt(n) => Value::BigInt(n),
            TLDbValue::TinyUnsigned(n) => Value::TinyUnsigned(n),
            TLDbValue::SmallUnsigned(n) => Value::SmallUnsigned(n),
            TLDbValue::Unsigned(n) => Value::Unsigned(n),
            TLDbValue::BigUnsigned(n) => Value::BigUnsigned(n),
            TLDbValue::Float(n) => Value::Float(n),
            TLDbValue::Double(n) => Value::Double(n),
            TLDbValue::String(n) => Value::String(n),
            TLDbValue::Char(n) => Value::Char(n),
            TLDbValue::Bytes(n) => Value::Bytes(n),
            TLDbValue::Json(n) => Value::Json(n),
            TLDbValue::ChronoDate(n) => Value::ChronoDate(n),
            TLDbValue::ChronoTime(n) => Value::ChronoTime(n),
            TLDbValue::ChronoDateTime(n) => Value::ChronoDateTime(n),
            TLDbValue::ChronoDateTimeUtc(n) => Value::ChronoDateTimeUtc(n),
            TLDbValue::ChronoDateTimeLocal(n) => Value::ChronoDateTimeLocal(n),
            TLDbValue::ChronoDateTimeWithTimeZone(n) => Value::ChronoDateTimeWithTimeZone(n),
            TLDbValue::Uuid(n) => Value::Uuid(n),
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug, Serialize, Deserialize, ToSchema)]
pub enum TLBinOp {
    And,
    Or,
    Like,
    NotLike,
    Is,
    IsNot,
    In,
    NotIn,
    Between,
    NotBetween,
    Equal,
    NotEqual,
    SmallerThan,
    GreaterThan,
    SmallerThanOrEqual,
    GreaterThanOrEqual,
}

impl From<TLBinOp> for BinOper {
    fn from(value: TLBinOp) -> Self {
        match value {
            TLBinOp::And => BinOper::And,
            TLBinOp::Or => BinOper::Or,
            TLBinOp::Like => BinOper::Like,
            TLBinOp::NotLike => BinOper::NotLike,
            TLBinOp::Is => BinOper::Is,
            TLBinOp::IsNot => BinOper::IsNot,
            TLBinOp::In => BinOper::In,
            TLBinOp::NotIn => BinOper::NotIn,
            TLBinOp::Between => BinOper::Between,
            TLBinOp::NotBetween => BinOper::NotBetween,
            TLBinOp::Equal => BinOper::Equal,
            TLBinOp::NotEqual => BinOper::NotEqual,
            TLBinOp::SmallerThan => BinOper::SmallerThan,
            TLBinOp::GreaterThan => BinOper::GreaterThan,
            TLBinOp::SmallerThanOrEqual => BinOper::SmallerThanOrEqual,
            TLBinOp::GreaterThanOrEqual => BinOper::GreaterThanOrEqual,
        }
    }
}

pub trait ApplyFilterOp {
    fn apply_filter_op(&self, value: Self, op: TLBinOp) -> bool;
}

impl ApplyFilterOp for u64 {
    fn apply_filter_op(&self, value: Self, op: TLBinOp) -> bool {
        match op {
            TLBinOp::And => self & value == value,
            TLBinOp::Or => self | value == value,
            TLBinOp::Like => self == &value,
            TLBinOp::NotLike => self != &value,
            TLBinOp::Is => self == &value,
            TLBinOp::IsNot => self != &value,
            TLBinOp::In => self == &value,
            TLBinOp::NotIn => self != &value,
            TLBinOp::Between => self == &value,
            TLBinOp::NotBetween => self != &value,
            TLBinOp::Equal => self == &value,
            TLBinOp::NotEqual => self != &value,
            TLBinOp::SmallerThan => self < &value,
            TLBinOp::GreaterThan => self > &value,
            TLBinOp::SmallerThanOrEqual => self <= &value,
            TLBinOp::GreaterThanOrEqual => self >= &value,
        }
    }
}

impl<'a> ApplyFilterOp for &'a str {
    fn apply_filter_op(&self, value: Self, op: TLBinOp) -> bool {
        match op {
            TLBinOp::And => true,
            TLBinOp::Or => true,
            TLBinOp::Like => self.contains(value),
            TLBinOp::NotLike => !self.contains(value),
            TLBinOp::Is => self == &value,
            TLBinOp::IsNot => self != &value,
            TLBinOp::In => value.contains(self),
            TLBinOp::NotIn => !value.contains(self),
            TLBinOp::Between => true,
            TLBinOp::NotBetween => true,
            TLBinOp::Equal => self == &value,
            TLBinOp::NotEqual => self != &value,
            TLBinOp::SmallerThan => self.len() < value.len(),
            TLBinOp::GreaterThan => self.len() > value.len(),
            TLBinOp::SmallerThanOrEqual => self.len() <= value.len(),
            TLBinOp::GreaterThanOrEqual => self.len() >= value.len(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSchema, Serialize, Deserialize)]
pub enum TLUnOp {
    Not,
}

impl From<TLUnOp> for UnOper {
    fn from(value: TLUnOp) -> Self {
        match value {
            TLUnOp::Not => UnOper::Not,
        }
    }
}

#[derive(Clone, Debug, ToSchema, Serialize, Deserialize)]
pub enum TLExpr {
    Unary(TLUnOp, Box<TLExpr>),
    Binary(Box<TLExpr>, TLBinOp, Box<TLExpr>),
    Column(String),
    Constant(TLDbValue),
    Value(TLDbValue),
    Values(Vec<TLDbValue>),
    // Tuple(Vec<SimpleExpr>),
    // Unary(UnOper, Box<SimpleExpr>),
    // FunctionCall(FunctionCall),
    // Binary(Box<SimpleExpr>, BinOper, Box<SimpleExpr>),
    // SubQuery(Option<SubQueryOper>, Box<SubQueryStatement>),
    // Value(Value),
    // Values(Vec<Value>),
    // Custom(String),
    // CustomWithExpr(String, Vec<SimpleExpr>),
    // Keyword(Keyword),
    // AsEnum(DynIden, Box<SimpleExpr>),
    // Case(Box<CaseStatement>),
    // Constant(Value),
}

impl From<TLExpr> for SimpleExpr {
    fn from(value: TLExpr) -> Self {
        match value {
            TLExpr::Unary(op, expr) => SimpleExpr::Unary(op.into(), Box::new((*expr).into())),
            TLExpr::Binary(lp, op, rp) => {
                SimpleExpr::Binary(Box::new((*lp).into()), op.into(), Box::new((*rp).into()))
            }
            TLExpr::Column(col) => {
                SimpleExpr::Column(ColumnRef::Column(DynIden::new(Alias::new(col))))
            }
            TLExpr::Constant(value) => SimpleExpr::Constant(value.into()),
            TLExpr::Value(value) => SimpleExpr::Value(value.into()),
            TLExpr::Values(values) => {
                SimpleExpr::Values(values.into_iter().map(|n| n.into()).collect())
            }
        }
    }
}

#[derive(Clone, Debug, ToSchema, Serialize, Deserialize)]
pub enum TlConditionItem {
    Condition(TLCondition),
    SimpleExpr(TLExpr),
}

impl Into<ConditionExpression> for TlConditionItem {
    fn into(self) -> ConditionExpression {
        match self {
            TlConditionItem::Condition(cond) => {
                ConditionExpression::Condition(cond.into_condition())
            }
            TlConditionItem::SimpleExpr(expr) => ConditionExpression::SimpleExpr(expr.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, ToSchema, Serialize, Deserialize)]
pub enum TLConditionType {
    Any,
    All,
}

#[derive(Clone, Debug, ToSchema, Serialize, Deserialize)]
pub struct TLCondition {
    pub negate: bool,
    pub condition_type: TLConditionType,
    pub conditions: Vec<TlConditionItem>,
}

impl IntoCondition for TLCondition {
    fn into_condition(self) -> Condition {
        let mut condition = match self.condition_type {
            TLConditionType::Any => Condition::any(),
            TLConditionType::All => Condition::all(),
        };
        for item in self.conditions {
            condition = condition.add(item);
        }
        if self.negate {
            condition.not()
        } else {
            condition
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema, PartialEq)]
pub enum TLStringLen {
    /// String size
    N(u32),
    Max,
    None,
}

impl From<StringLen> for TLStringLen {
    fn from(value: StringLen) -> Self {
        match value {
            StringLen::N(n) => TLStringLen::N(n),
            StringLen::Max => TLStringLen::Max,
            StringLen::None => TLStringLen::None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema, PartialEq)]
pub enum TLColumnType {
    Char(Option<u32>),
    String(TLStringLen),
    Text,
    Blob,
    TinyInteger,
    SmallInteger,
    Integer,
    BigInteger,
    TinyUnsigned,
    SmallUnsigned,
    Unsigned,
    BigUnsigned,
    Float,
    Double,
    Decimal(Option<(u32, u32)>),
    DateTime,
    Timestamp,
    TimestampWithTimeZone,
    Time,
    Date,
    Year,
    // Interval(Option<PgInterval>, Option<u32>),
    Binary(u32),
    // VarBinary(TLStringLen),
    Bit(Option<u32>),
    VarBit(u32),
    Boolean,
    Money(Option<(u32, u32)>),
    Json,
    JsonBinary,
    Uuid,
    // Custom(DynIden),
    // Enum {
    //     name: DynIden,
    //     variants: Vec<DynIden>,
    // },
    // Array(RcOrArc<sea_orm::ColumnType>),
    Cidr,
    Inet,
    MacAddr,
    LTree,
}

impl From<ColumnType> for TLColumnType {
    fn from(value: ColumnType) -> Self {
        match value {
            ColumnType::Char(n) => TLColumnType::Char(n),
            ColumnType::String(n) => TLColumnType::String(n.into()),
            ColumnType::Text => TLColumnType::Text,
            ColumnType::Blob => TLColumnType::Blob,
            ColumnType::TinyInteger => TLColumnType::TinyInteger,
            ColumnType::SmallInteger => TLColumnType::SmallInteger,
            ColumnType::Integer => TLColumnType::Integer,
            ColumnType::BigInteger => TLColumnType::BigInteger,
            ColumnType::TinyUnsigned => TLColumnType::TinyUnsigned,
            ColumnType::SmallUnsigned => TLColumnType::SmallUnsigned,
            ColumnType::Unsigned => TLColumnType::Unsigned,
            ColumnType::BigUnsigned => TLColumnType::BigUnsigned,
            ColumnType::Float => TLColumnType::Float,
            ColumnType::Double => TLColumnType::Double,
            // ColumnType::Decimal(_) => TLColumnType::Decimal,
            ColumnType::DateTime => TLColumnType::DateTime,
            ColumnType::Timestamp => TLColumnType::Timestamp,
            ColumnType::TimestampWithTimeZone => TLColumnType::TimestampWithTimeZone,
            ColumnType::Time => TLColumnType::Time,
            ColumnType::Date => TLColumnType::Date,
            ColumnType::Year => TLColumnType::Year,
            // ColumnType::Interval(a, b) => TLColumnType::Interval,
            ColumnType::Binary(n) => TLColumnType::Binary(n),
            // ColumnType::VarBinary(n) => TLColumnType::VarBinary(n.into()),
            ColumnType::Bit(n) => TLColumnType::Bit(n),
            ColumnType::VarBit(n) => TLColumnType::VarBit(n),
            ColumnType::Boolean => TLColumnType::Boolean,
            ColumnType::Money(n) => TLColumnType::Money(n),
            ColumnType::Json => TLColumnType::Json,
            ColumnType::JsonBinary => TLColumnType::JsonBinary,
            ColumnType::Uuid => TLColumnType::Uuid,
            // ColumnType::Custom(n) => TLColumnType::Custom(n),
            // ColumnType::Enum { .. } => TLColumnType::Enum,
            // ColumnType::Array(n) => TLColumnType::Array(n),
            ColumnType::Cidr => TLColumnType::Cidr,
            ColumnType::Inet => TLColumnType::Inet,
            ColumnType::MacAddr => TLColumnType::MacAddr,
            ColumnType::LTree => TLColumnType::LTree,
            _ => unimplemented!(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, IntoParams, ToSchema)]
pub struct TableColumnInfo {
    pub name: Cow<'static, str>,
    pub column_type: TLColumnType,
}

#[derive(Default, Clone, Debug, Deserialize, ToSchema)]
pub struct TableInfo {
    pub name: Cow<'static, str>,
    pub columns: Vec<TableColumnInfo>,
}
