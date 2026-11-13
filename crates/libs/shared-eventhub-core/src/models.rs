#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MessageKey {                 // uniqueness = partition + sequence
    pub partition_id: String,
    pub sequence_number: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BodyKind { Utf8, LossyUtf8, Missing }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapturedMessage {
    pub key: MessageKey,
    pub offset: Option<String>,         // opaque string in the new SDK
    pub partition_key: Option<String>,
    pub enqueued_time: Option<DateTime<Utc>>,
    pub processed_at: DateTime<Utc>,
    pub message_id: Option<String>,     // MessageId enum rendered to string
    pub correlation_id: Option<String>,
    pub content_type: Option<String>,
    pub properties: BTreeMap<String, String>,  // app properties, values stringified
    pub body: String,
    pub body_kind: BodyKind,
}