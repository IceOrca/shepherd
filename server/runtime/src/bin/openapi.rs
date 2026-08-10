fn main() {
    let json: String =
        serde_json::to_string_pretty(&shepherd_runtime::api_document()).expect("OpenAPI document should serialize");
    println!("{json}");
}
