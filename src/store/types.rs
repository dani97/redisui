use redis::Value;

pub fn decode(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (hex_preview(bytes), true),
    }
}

pub fn hex_preview(bytes: &[u8]) -> String {
    const SHOWN: usize = 48;
    let shown = bytes.len().min(SHOWN);
    let mut out = String::with_capacity(shown * 2 + 8);
    for byte in &bytes[..shown] {
        out.push_str(&format!("{byte:02x}"));
    }
    if bytes.len() > shown {
        out.push_str("..");
    }
    out
}

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::BulkString(bytes) => decode(bytes).0,
        Value::SimpleString(text) => text.clone(),
        Value::Int(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Okay => "OK".to_string(),
        Value::VerbatimString { text, .. } => text.clone(),
        _ => String::new(),
    }
}

pub fn value_to_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::BulkString(bytes) => bytes.clone(),
        other => value_to_string(other).into_bytes(),
    }
}

pub fn value_to_array(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        _ => Vec::new(),
    }
}

pub fn cursor_reply(reply: &Value) -> (u64, Vec<Vec<u8>>) {
    let items = value_to_array(reply);
    let cursor = items
        .first()
        .map(value_to_string)
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let names = items
        .get(1)
        .map(value_to_array)
        .unwrap_or_default()
        .iter()
        .map(value_to_bytes)
        .collect();
    (cursor, names)
}

pub fn scan_pairs(reply: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    value_to_array(reply)
        .get(1)
        .map(value_to_array)
        .unwrap_or_default()
        .chunks(2)
        .filter(|chunk| chunk.len() == 2)
        .map(|chunk| (value_to_bytes(&chunk[0]), value_to_bytes(&chunk[1])))
        .collect()
}

pub fn scan_members(reply: &Value) -> Vec<Vec<u8>> {
    value_to_array(reply)
        .get(1)
        .map(value_to_array)
        .unwrap_or_default()
        .iter()
        .map(value_to_bytes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulk(text: &str) -> Value {
        Value::BulkString(text.as_bytes().to_vec())
    }

    #[test]
    fn decodes_utf8_without_binary_flag() {
        let (text, binary) = decode("héllo".as_bytes());
        assert_eq!(text, "héllo");
        assert!(!binary);
    }

    #[test]
    fn falls_back_to_hex_for_invalid_utf8() {
        let (text, binary) = decode(&[0xff, 0xfe, 0x00, 0x41]);
        assert!(binary);
        assert_eq!(text, "fffe0041");
    }

    #[test]
    fn hex_preview_truncates_long_values() {
        let bytes = vec![0xab; 100];
        let preview = hex_preview(&bytes);
        assert!(preview.ends_with(".."));
        assert_eq!(preview.len(), 48 * 2 + 2);
    }

    #[test]
    fn parses_scan_cursor_and_keys() {
        let reply = Value::Array(vec![bulk("17"), Value::Array(vec![bulk("a"), bulk("b")])]);
        let (cursor, keys) = cursor_reply(&reply);
        assert_eq!(cursor, 17);
        assert_eq!(keys, vec![b"a".to_vec(), b"b".to_vec()]);
    }

    #[test]
    fn treats_missing_cursor_as_complete() {
        let reply = Value::Array(vec![bulk("0"), Value::Array(vec![])]);
        let (cursor, keys) = cursor_reply(&reply);
        assert_eq!(cursor, 0);
        assert!(keys.is_empty());
    }

    #[test]
    fn parses_hash_scan_pairs() {
        let reply = Value::Array(vec![
            bulk("0"),
            Value::Array(vec![bulk("f1"), bulk("v1"), bulk("f2"), bulk("v2")]),
        ]);
        let pairs = scan_pairs(&reply);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (b"f1".to_vec(), b"v1".to_vec()));
        assert_eq!(pairs[1], (b"f2".to_vec(), b"v2".to_vec()));
    }

    #[test]
    fn parses_set_scan_members() {
        let reply = Value::Array(vec![bulk("0"), Value::Array(vec![bulk("x"), bulk("y")])]);
        let members = scan_members(&reply);
        assert_eq!(members, vec![b"x".to_vec(), b"y".to_vec()]);
    }

    #[test]
    fn renders_status_and_int_replies() {
        assert_eq!(value_to_string(&Value::SimpleString("zset".into())), "zset");
        assert_eq!(value_to_string(&Value::Int(7)), "7");
        assert_eq!(value_to_string(&Value::Okay), "OK");
    }
}
