use crate::{Store, Value, data_types::Value};

pub async fn handle_hset(
    store: &Store,
    key: String,
    field: String,
    value: String,
) -> Result<i64, String> {
    let mut store_data = store.data.write().await;
    
    let entry = store_data.entry(key).or_insert(Value::Hash(HashMap::new()));
    
    match entry {
        Value::Hash(hash) => {
            let replaced = hash.insert(field, value).is_some();
            Ok(if replaced { 0 } else { 1 })
        }
        _ => Err("ERR Operation against wrong type".to_string()),
    }
}