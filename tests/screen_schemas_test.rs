//! Every bundled screen's @params block must parse without error, and known
//! screens expose their documented params.

use byonk::assets::AssetLoader;
use byonk::models::param_schema::schema_for_script;
use std::path::Path;

fn schema(script: &str) -> Option<Vec<String>> {
    let loader = AssetLoader::new(None, None, None);
    let src = loader.read_screen_string(Path::new(script)).unwrap();
    schema_for_script(&src)
        .expect("schema must parse")
        .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
}

#[test]
fn test_transit_params() {
    let names = schema("transit.lua").unwrap();
    assert!(names.contains(&"station".to_string()));
    assert!(names.contains(&"limit".to_string()));
}

#[test]
fn test_gphoto_params() {
    let names = schema("gphoto.lua").unwrap();
    assert!(names.contains(&"album_url".to_string()));
}

#[test]
fn test_fontdemo_bitmap_is_enum() {
    let loader = AssetLoader::new(None, None, None);
    let src = loader
        .read_screen_string(Path::new("fontdemo-bitmap.lua"))
        .unwrap();
    let schema = schema_for_script(&src).unwrap().unwrap();
    let f = schema
        .fields
        .iter()
        .find(|f| f.name == "font_prefix")
        .unwrap();
    assert!(!f.options.is_empty());
}

#[test]
fn test_no_param_screens_have_no_schema_or_empty() {
    for s in ["default.lua", "graytest.lua", "hello.lua", "mandelbrot.lua"] {
        let loader = AssetLoader::new(None, None, None);
        let src = loader.read_screen_string(Path::new(s)).unwrap();
        // Either no block, or a block that parses to zero fields.
        let parsed = schema_for_script(&src).expect("must parse");
        if let Some(p) = parsed {
            assert!(p.fields.is_empty());
        }
    }
}
