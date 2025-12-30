// Arc<Html> is used in single-threaded Lua context, so Send+Sync not required
#![allow(clippy::arc_with_non_send_sync)]

use mlua::{Lua, Result as LuaResult, Table, UserData, UserDataMethods, Value};
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::sync::Arc;

use super::DeviceContext;

/// Result from running a Lua script
#[derive(Debug)]
pub struct ScriptResult {
    /// Data to pass to the template
    pub data: serde_json::Value,
    /// Refresh rate in seconds
    pub refresh_rate: u32,
    /// If true, skip rendering and just tell device to check back later
    pub skip_update: bool,
}

/// Error type for Lua script execution
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    #[error("Script not found: {0}")]
    NotFound(String),
}

/// Lua runtime for executing screen scripts
pub struct LuaRuntime {
    screens_dir: std::path::PathBuf,
}

impl LuaRuntime {
    pub fn new(screens_dir: &std::path::Path) -> Self {
        Self {
            screens_dir: screens_dir.to_path_buf(),
        }
    }

    /// Run a Lua script with the given parameters
    pub fn run_script(
        &self,
        script_path: &std::path::Path,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
    ) -> Result<ScriptResult, ScriptError> {
        let full_path = self.screens_dir.join(script_path);

        let script_content = std::fs::read_to_string(&full_path)
            .map_err(|_| ScriptError::NotFound(full_path.display().to_string()))?;

        let lua = Lua::new();

        // Set up the Lua environment
        self.setup_globals(&lua, params, device_ctx)?;

        // Execute the script
        let result: Table = lua.load(&script_content).eval()?;

        // Extract data, refresh_rate, and skip_update
        let data = self.table_to_json(&lua, result.get::<Table>("data")?)?;
        let refresh_rate: u32 = result.get("refresh_rate").unwrap_or(900);
        let skip_update: bool = result.get("skip_update").unwrap_or(false);

        Ok(ScriptResult {
            data,
            refresh_rate,
            skip_update,
        })
    }

    /// Set up Lua global functions and variables
    fn setup_globals(
        &self,
        lua: &Lua,
        params: &HashMap<String, serde_yaml::Value>,
        device_ctx: Option<&DeviceContext>,
    ) -> LuaResult<()> {
        let globals = lua.globals();

        // Add params table
        let params_table = lua.create_table()?;
        for (key, value) in params {
            params_table.set(key.as_str(), self.yaml_to_lua(lua, value)?)?;
        }
        globals.set("params", params_table)?;

        // Add device table
        let device_table = lua.create_table()?;
        if let Some(ctx) = device_ctx {
            device_table.set("mac", ctx.mac.as_str())?;
            if let Some(voltage) = ctx.battery_voltage {
                device_table.set("battery_voltage", voltage)?;
            }
            if let Some(rssi) = ctx.rssi {
                device_table.set("rssi", rssi)?;
            }
        }
        globals.set("device", device_table)?;

        // http_get(url) -> string
        let http_get = lua.create_function(|_, url: String| {
            tracing::debug!(url = %url, "Lua http_get");
            match reqwest::blocking::get(&url) {
                Ok(response) => match response.text() {
                    Ok(text) => Ok(text),
                    Err(e) => Err(mlua::Error::external(format!(
                        "Failed to read response: {e}"
                    ))),
                },
                Err(e) => Err(mlua::Error::external(format!("HTTP request failed: {e}"))),
            }
        })?;
        globals.set("http_get", http_get)?;

        // html_parse(html) -> Document
        let html_parse = lua.create_function(|_, html: String| {
            Ok(LuaDocument {
                doc: Arc::new(Html::parse_document(&html)),
            })
        })?;
        globals.set("html_parse", html_parse)?;

        // time_now() -> number (Unix timestamp)
        let time_now = lua.create_function(|_, ()| Ok(chrono::Utc::now().timestamp()))?;
        globals.set("time_now", time_now)?;

        // time_format(timestamp, format) -> string (uses local time)
        let time_format = lua.create_function(|_, (ts, fmt): (i64, String)| {
            use chrono::{Local, TimeZone};
            let dt = Local
                .timestamp_opt(ts, 0)
                .single()
                .ok_or_else(|| mlua::Error::external("Invalid timestamp"))?;
            Ok(dt.format(&fmt).to_string())
        })?;
        globals.set("time_format", time_format)?;

        // time_parse(str, format) -> number
        let time_parse = lua.create_function(|_, (s, fmt): (String, String)| {
            use chrono::NaiveDateTime;
            let dt = NaiveDateTime::parse_from_str(&s, &fmt)
                .map_err(|e| mlua::Error::external(format!("Failed to parse time: {e}")))?;
            Ok(dt.and_utc().timestamp())
        })?;
        globals.set("time_parse", time_parse)?;

        // json_decode(json_string) -> table
        let json_decode = lua.create_function(|lua, json_str: String| {
            let value: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| mlua::Error::external(format!("JSON parse error: {e}")))?;
            json_to_lua(lua, &value)
        })?;
        globals.set("json_decode", json_decode)?;

        // json_encode(table) -> string
        let json_encode = lua.create_function(|lua, value: Value| {
            let json = lua_value_to_json(lua, value)?;
            serde_json::to_string(&json)
                .map_err(|e| mlua::Error::external(format!("JSON encode error: {e}")))
        })?;
        globals.set("json_encode", json_encode)?;

        // Logging functions
        let log_info = lua.create_function(|_, msg: String| {
            tracing::info!(script = true, "{}", msg);
            Ok(())
        })?;
        globals.set("log_info", log_info)?;

        let log_warn = lua.create_function(|_, msg: String| {
            tracing::warn!(script = true, "{}", msg);
            Ok(())
        })?;
        globals.set("log_warn", log_warn)?;

        let log_error = lua.create_function(|_, msg: String| {
            tracing::error!(script = true, "{}", msg);
            Ok(())
        })?;
        globals.set("log_error", log_error)?;

        Ok(())
    }

    /// Convert a Lua table to JSON
    fn table_to_json(&self, lua: &Lua, table: Table) -> LuaResult<serde_json::Value> {
        self.lua_to_json(lua, Value::Table(table))
    }

    /// Convert a Lua value to JSON
    #[allow(clippy::only_used_in_recursion)]
    fn lua_to_json(&self, lua: &Lua, value: Value) -> LuaResult<serde_json::Value> {
        match value {
            Value::Nil => Ok(serde_json::Value::Null),
            Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
            Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
            Value::Number(n) => Ok(serde_json::json!(n)),
            Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
            Value::Table(t) => {
                // Check if it's an array (sequential integer keys starting at 1)
                let len = t.raw_len();
                if len > 0 {
                    let mut arr = Vec::new();
                    for i in 1..=len {
                        if let Ok(v) = t.raw_get::<Value>(i) {
                            arr.push(self.lua_to_json(lua, v)?);
                        }
                    }
                    // Verify it's really an array by checking key count
                    let mut key_count = 0;
                    for _ in t.clone().pairs::<Value, Value>() {
                        key_count += 1;
                    }
                    if key_count == len {
                        return Ok(serde_json::Value::Array(arr));
                    }
                }

                // It's an object
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, self.lua_to_json(lua, v)?);
                }
                Ok(serde_json::Value::Object(map))
            }
            Value::UserData(ud) => {
                // Try to extract meaningful data from userdata
                if ud.is::<LuaElement>() {
                    let elem = ud.borrow::<LuaElement>()?;
                    Ok(serde_json::Value::String(elem.text()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    /// Convert YAML value to Lua value
    #[allow(clippy::only_used_in_recursion)]
    fn yaml_to_lua(&self, lua: &Lua, value: &serde_yaml::Value) -> LuaResult<Value> {
        match value {
            serde_yaml::Value::Null => Ok(Value::Nil),
            serde_yaml::Value::Bool(b) => Ok(Value::Boolean(*b)),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Number(f))
                } else {
                    Ok(Value::Nil)
                }
            }
            serde_yaml::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
            serde_yaml::Value::Sequence(arr) => {
                let table = lua.create_table()?;
                for (i, v) in arr.iter().enumerate() {
                    table.set(i + 1, self.yaml_to_lua(lua, v)?)?;
                }
                Ok(Value::Table(table))
            }
            serde_yaml::Value::Mapping(map) => {
                let table = lua.create_table()?;
                for (k, v) in map {
                    if let serde_yaml::Value::String(key) = k {
                        table.set(key.as_str(), self.yaml_to_lua(lua, v)?)?;
                    }
                }
                Ok(Value::Table(table))
            }
            _ => Ok(Value::Nil),
        }
    }
}

/// Wrapper for scraper's Html document exposed to Lua
#[derive(Clone)]
struct LuaDocument {
    doc: Arc<Html>,
}

impl UserData for LuaDocument {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // select(selector) -> Elements
        methods.add_method("select", |lua, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            let elements: Vec<_> = this
                .doc
                .select(&sel)
                .map(|el| LuaElement::new(el.html()))
                .collect();

            let table = lua.create_table()?;
            for (i, elem) in elements.into_iter().enumerate() {
                table.set(i + 1, elem)?;
            }

            // Add each() method to the table
            // Use raw_len and raw_get to iterate only over array elements (not the "each" key)
            let each_fn = lua.create_function(|_, (tbl, func): (Table, mlua::Function)| {
                let len = tbl.raw_len();
                for i in 1..=len {
                    if let Ok(elem) = tbl.raw_get::<Value>(i) {
                        func.call::<()>(elem)?;
                    }
                }
                Ok(())
            })?;
            table.set("each", each_fn)?;

            Ok(table)
        });

        // select_one(selector) -> Element or nil
        methods.add_method("select_one", |_, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            Ok(this
                .doc
                .select(&sel)
                .next()
                .map(|el| LuaElement::new(el.html())))
        });
    }
}

/// Wrapper for a single HTML element exposed to Lua
#[derive(Clone)]
struct LuaElement {
    html: String,
}

impl LuaElement {
    fn new(html: String) -> Self {
        Self { html }
    }

    fn text(&self) -> String {
        let fragment = Html::parse_fragment(&self.html);
        fragment
            .root_element()
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string()
    }

    fn get_attr(&self, name: &str) -> Option<String> {
        let fragment = Html::parse_fragment(&self.html);
        fragment
            .root_element()
            .select(&Selector::parse("*").unwrap())
            .next()
            .and_then(|el| el.value().attr(name).map(|s| s.to_string()))
    }
}

impl UserData for LuaElement {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // text() -> string
        methods.add_method("text", |_, this, ()| Ok(this.text()));

        // attr(name) -> string or nil
        methods.add_method("attr", |_, this, name: String| Ok(this.get_attr(&name)));

        // html() -> string
        methods.add_method("html", |_, this, ()| Ok(this.html.clone()));

        // select(selector) -> Elements (for chaining)
        methods.add_method("select", |lua, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            // Parse as fragment and search all elements (not just from root)
            let fragment = Html::parse_fragment(&this.html);
            let elements: Vec<_> = fragment
                .select(&sel)
                .map(|el| LuaElement::new(el.html()))
                .collect();

            let table = lua.create_table()?;
            for (i, elem) in elements.into_iter().enumerate() {
                table.set(i + 1, elem)?;
            }

            // Add each() method
            // Use raw_len and raw_get to iterate only over array elements (not the "each" key)
            let each_fn = lua.create_function(|_, (tbl, func): (Table, mlua::Function)| {
                let len = tbl.raw_len();
                for i in 1..=len {
                    if let Ok(elem) = tbl.raw_get::<Value>(i) {
                        func.call::<()>(elem)?;
                    }
                }
                Ok(())
            })?;
            table.set("each", each_fn)?;

            Ok(table)
        });

        // select_one(selector) -> Element or nil
        methods.add_method("select_one", |_, this, selector: String| {
            let sel = Selector::parse(&selector)
                .map_err(|e| mlua::Error::external(format!("Invalid selector: {e:?}")))?;

            // Parse as fragment and search all elements (not just from root)
            let fragment = Html::parse_fragment(&this.html);
            Ok(fragment
                .select(&sel)
                .next()
                .map(|el| LuaElement::new(el.html())))
        });
    }
}

/// Convert JSON value to Lua value
fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Convert Lua value to JSON (standalone function for use in closures)
fn lua_value_to_json(_lua: &Lua, value: Value) -> LuaResult<serde_json::Value> {
    lua_to_json_inner(value)
}

/// Inner conversion function that doesn't need Lua reference
fn lua_to_json_inner(value: Value) -> LuaResult<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
        Value::Number(n) => Ok(serde_json::json!(n)),
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting at 1)
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::new();
                for i in 1..=len {
                    if let Ok(v) = t.raw_get::<Value>(i) {
                        arr.push(lua_to_json_inner(v)?);
                    }
                }
                // Verify it's really an array by checking key count
                let mut key_count = 0;
                for _ in t.clone().pairs::<Value, Value>() {
                    key_count += 1;
                }
                if key_count == len {
                    return Ok(serde_json::Value::Array(arr));
                }
            }

            // It's an object
            let mut map = serde_json::Map::new();
            for pair in t.pairs::<String, Value>() {
                let (k, v) = pair?;
                map.insert(k, lua_to_json_inner(v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Ok(serde_json::Value::Null),
    }
}
