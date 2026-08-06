//! The published meta.yaml schema must describe what the parser actually
//! accepts — it is served to LLM authors as a contract.

use byonk::models::screen_meta::{meta_json_schema, ScreenMeta};

#[test]
fn test_schema_requires_exactly_what_the_parser_requires() {
    let schema = meta_json_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("schema must declare required fields")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // These three are the ones from_yaml rejects a document for missing.
    assert!(required.contains(&"title"));
    assert!(required.contains(&"description"));
    assert!(required.contains(&"byonk"));
    assert!(!required.contains(&"refresh"), "refresh is optional");
    assert!(!required.contains(&"params"), "params is optional");
}

#[test]
fn test_schema_documents_every_optional_top_level_field() {
    let schema = meta_json_schema();
    let props = schema["properties"].as_object().unwrap();
    for f in ["title", "description", "byonk", "refresh", "params"] {
        assert!(props.contains_key(f), "schema is missing property {f}");
    }
}

#[test]
fn test_parser_agrees_with_the_schemas_required_set() {
    // Guard against drift in the other direction: a document with only the
    // required fields must parse.
    let minimal = "title: t\ndescription: d\nbyonk: \"0.17\"\n";
    assert!(ScreenMeta::from_yaml(minimal).is_ok());

    // And dropping any one of them must fail.
    for drop in ["title: t\n", "description: d\n", "byonk: \"0.17\"\n"] {
        let src = minimal.replace(drop, "");
        assert!(
            ScreenMeta::from_yaml(&src).is_err(),
            "parser accepted a document missing a schema-required field: {src}"
        );
    }
}

#[test]
fn test_params_schema_describes_the_field_descriptor() {
    let schema = meta_json_schema();
    let text = serde_json::to_string(&schema).unwrap();
    // The params sub-language is the part an author most needs spelled out.
    for token in ["type", "required", "options", "label"] {
        assert!(
            text.contains(token),
            "params descriptor is missing `{token}` in the published schema"
        );
    }
}
