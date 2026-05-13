pub mod presets;
pub mod start;
pub mod stop;
pub mod color;

pub fn print_ok_json(extra: serde_json::Value) {
    let mut obj = serde_json::Map::new();
    obj.insert("ok".into(), serde_json::Value::Bool(true));
    if let serde_json::Value::Object(map) = extra {
        for (k, v) in map { obj.insert(k, v); }
    }
    println!("{}", serde_json::to_string(&serde_json::Value::Object(obj)).unwrap());
}
