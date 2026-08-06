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

/// Cleanup 9: the generated schema's top-level `title` defaulted to the
/// Rust-internal type name `RawMeta`, which means nothing to the LLM authors
/// this document is published to (`byonk://schema/meta.yaml`).
#[test]
fn test_schema_title_is_meta_yaml_not_the_rust_type_name() {
    let schema = meta_json_schema();
    assert_eq!(schema["title"], "meta.yaml");
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

/// `parse_options` (src/models/param_schema.rs) accepts an `options` entry
/// that is EITHER a bare string OR a `{value, label}` map with `label`
/// optional. A schema that requires both `value` and `label` on every entry
/// (as plain `EnumOption` does) is narrower than the parser and would reject
/// documents byonk actually accepts — the "schema drifts into a lie" failure
/// this whole feature exists to prevent, so pin the union shape directly.
#[test]
fn test_options_schema_accepts_both_bare_strings_and_maps_with_optional_label() {
    let schema = meta_json_schema();

    // schema.properties.params.additionalProperties -> {"$ref": "#/$defs/RawField"}
    let defs = schema["$defs"].as_object().expect("schema must have $defs");
    let raw_field = &defs["RawField"];
    let options_prop = &raw_field["properties"]["options"];
    // options: Option<Vec<RawEnumOption>> -> {"type": ["array","null"], "items": {"$ref": "#/$defs/RawEnumOption"}}
    let items_ref = options_prop["items"]["$ref"]
        .as_str()
        .expect("options.items must be a $ref to the enum-option union type");
    let ref_name = items_ref
        .strip_prefix("#/$defs/")
        .expect("expected a local $defs $ref");
    let raw_enum_option = &defs[ref_name];

    let branches = raw_enum_option["anyOf"]
        .as_array()
        .or_else(|| raw_enum_option["oneOf"].as_array())
        .expect(
            "enum-option union must be anyOf/oneOf branches, not a single required-fields object",
        );

    let has_bare_string_branch = branches
        .iter()
        .any(|b| b["type"].as_str() == Some("string"));
    assert!(
        has_bare_string_branch,
        "enum-option schema is missing the bare-string branch: {branches:?}"
    );

    let has_object_branch_with_only_value_required = branches.iter().any(|b| {
        b["type"].as_str() == Some("object")
            && b["required"].as_array().is_some_and(|r| {
                let names: Vec<&str> = r.iter().filter_map(|v| v.as_str()).collect();
                names.contains(&"value") && !names.contains(&"label")
            })
    });
    assert!(
        has_object_branch_with_only_value_required,
        "enum-option schema's object branch must require only `value`, not `label`: {branches:?}"
    );
}
