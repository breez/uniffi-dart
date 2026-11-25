// Records and the variant classes of enums with fields compare by value in
// Dart, as in UniFFI's other bindings. Every value here also round-trips
// through Rust, so the tests compare a Dart-built value with a freshly lifted
// copy that shares no lists, maps or byte arrays with it.

use std::collections::HashMap;
use std::sync::Arc;

#[derive(uniffi::Record)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(uniffi::Record)]
pub struct Payload {
    pub name: String,
    pub data: Vec<u8>,
    pub tags: Vec<String>,
    pub attrs: HashMap<String, i64>,
    pub note: Option<String>,
    pub ratio: f64,
}

#[derive(uniffi::Record)]
pub struct Nested {
    pub point: Point,
    pub points: Vec<Point>,
    pub by_name: HashMap<String, Point>,
    pub grid: Vec<Vec<u8>>,
    pub shape: Shape,
    pub maybe: Option<Point>,
}

// Dart compares `Uint8List` keys by identity, so equal maps only find each
// other's keys by their contents.
#[derive(uniffi::Record)]
pub struct ByteIndex {
    pub by_bytes: HashMap<Vec<u8>, String>,
}

// A field named `other` is shadowed by the parameter of `==`.
#[derive(uniffi::Record)]
pub struct Pair {
    pub value: i32,
    pub other: i32,
}

#[derive(uniffi::Record)]
pub struct Empty {}

#[derive(uniffi::Enum)]
pub enum Shape {
    Circle { radius: f64 },
    Rect { width: f64, height: f64 },
    Dot,
}

#[derive(uniffi::Enum)]
pub enum Event {
    Bytes(Vec<u8>),
    Batch { items: Vec<Point> },
    Labels { by_id: HashMap<u32, String> },
    Moved(Point, Shape),
    Idle,
    Stopped,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LookupError {
    #[error("not found: {key}")]
    NotFound { key: String },
    #[error("invalid codes")]
    Invalid { codes: Vec<u32> },
    #[error("unknown")]
    Unknown,
}

/// No `Eq` export, so it keeps identity.
#[derive(uniffi::Object)]
pub struct Counter {
    pub start: u32,
}

#[uniffi::export]
impl Counter {
    #[uniffi::constructor]
    pub fn new(start: u32) -> Arc<Self> {
        Arc::new(Self { start })
    }
}

/// Exports `Eq` and `Hash`, so it compares by Rust's equality.
#[derive(PartialEq, Eq, Hash, uniffi::Object)]
#[uniffi::export(Eq, Hash)]
pub struct Key {
    pub id: u32,
}

#[uniffi::export]
impl Key {
    #[uniffi::constructor]
    pub fn new(id: u32) -> Arc<Self> {
        Arc::new(Self { id })
    }
}

#[derive(uniffi::Record)]
pub struct Holder {
    pub counter: Arc<Counter>,
    pub label: String,
}

#[derive(uniffi::Record)]
pub struct KeyHolder {
    pub key: Arc<Key>,
    pub keys: Vec<Arc<Key>>,
}

#[derive(uniffi::Enum)]
pub enum Owner {
    Counted(Arc<Counter>),
    Keyed(Arc<Key>),
}

#[uniffi::export]
pub fn roundtrip_point(value: Point) -> Point {
    value
}

#[uniffi::export]
pub fn roundtrip_points(value: Vec<Point>) -> Vec<Point> {
    value
}

#[uniffi::export]
pub fn roundtrip_payload(value: Payload) -> Payload {
    value
}

#[uniffi::export]
pub fn roundtrip_nested(value: Nested) -> Nested {
    value
}

#[uniffi::export]
pub fn roundtrip_byte_index(value: ByteIndex) -> ByteIndex {
    value
}

#[uniffi::export]
pub fn roundtrip_pair(value: Pair) -> Pair {
    value
}

#[uniffi::export]
pub fn roundtrip_empty(value: Empty) -> Empty {
    value
}

#[uniffi::export]
pub fn roundtrip_shape(value: Shape) -> Shape {
    value
}

#[uniffi::export]
pub fn roundtrip_event(value: Event) -> Event {
    value
}

#[uniffi::export]
pub fn roundtrip_holder(value: Holder) -> Holder {
    value
}

#[uniffi::export]
pub fn roundtrip_key_holder(value: KeyHolder) -> KeyHolder {
    value
}

#[uniffi::export]
pub fn roundtrip_owner(value: Owner) -> Owner {
    value
}

#[uniffi::export]
pub fn lookup(key: String) -> Result<u32, LookupError> {
    match key.as_str() {
        "" => Err(LookupError::Unknown),
        "bad" => Err(LookupError::Invalid { codes: vec![1, 2, 3] }),
        _ => Err(LookupError::NotFound { key }),
    }
}

uniffi::include_scaffolding!("api");
