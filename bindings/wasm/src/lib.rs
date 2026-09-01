//! WebAssembly bindings for `blazingly-aasa`.
//!
//! The compiled document stays inside WebAssembly. JavaScript holds a handle and crosses the
//! boundary only with small arguments and small results, which is what keeps this fast: shipping a
//! whole parsed association file across the boundary as a JS object tree would cost more than the
//! parse itself.
//!
//! ```js
//! import init, { Aasa } from "@blazingly/aasa";
//!
//! await init();
//! const aasa = Aasa.compile(bytes, "example.com");
//! try {
//!   console.log(aasa.decide(appId, url));   // "match" | "exclude" | "no_match"
//!   console.log(aasa.explain(appId, url));  // human-readable trace
//! } finally {
//!   aasa.free();
//! }
//! ```

use blazingly_aasa::{CompiledAasa, MatchDecision, ParseOptions};
use wasm_bindgen::prelude::*;

/// Installs a panic hook that reports Rust panics through `console.error`.
///
/// Optional, and only useful while debugging.
#[wasm_bindgen(js_name = setPanicHook)]
pub fn set_panic_hook() {
    // A panic here would otherwise surface as an opaque "unreachable executed".
    std::panic::set_hook(Box::new(|info| {
        web_error(&info.to_string());
    }));
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn web_error(message: &str);
}

fn to_js<T: serde::Serialize + ?Sized>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsError::new(&error.to_string()))
}

/// A parsed and compiled association file.
///
/// Call `free()` from JavaScript when finished; the handle owns memory inside the
/// WebAssembly instance.
#[wasm_bindgen]
pub struct Aasa {
    inner: CompiledAasa,
    domain: String,
}

#[wasm_bindgen]
impl Aasa {
    /// Parses and compiles `bytes`.
    ///
    /// `domain` is the host the file was served for; matching rejects URLs on any other host.
    /// Pass an empty string to skip that check.
    ///
    /// # Errors
    ///
    /// Throws for invalid JSON, a non-object root, or a payload above `maxBytes`.
    #[wasm_bindgen]
    pub fn compile(bytes: &[u8], domain: &str, max_bytes: Option<usize>) -> Result<Aasa, JsError> {
        let options = match max_bytes {
            Some(limit) => ParseOptions::new().max_bytes(limit),
            None => ParseOptions::default(),
        };
        let inner = CompiledAasa::parse_with(bytes, &options)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(Self {
            inner,
            domain: domain.to_owned(),
        })
    }

    /// The domain this handle matches against.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn domain(&self) -> String {
        self.domain.clone()
    }

    /// Every diagnostic, as an array of `{ code, severity, path, message, help }`.
    ///
    /// # Errors
    ///
    /// Throws if the report cannot be converted to a JavaScript value.
    #[wasm_bindgen]
    pub fn validate(&self) -> Result<JsValue, JsError> {
        to_js(self.inner.validate().diagnostics())
    }

    /// Whether the document reports any error-severity diagnostic.
    #[wasm_bindgen(js_name = hasErrors)]
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.inner.validate().has_errors()
    }

    /// The decision alone: `"match"`, `"exclude"`, or `"no_match"`.
    ///
    /// This is the cheap call. Use it in a loop; use [`Aasa::match_url`] when a human needs to
    /// understand the answer.
    ///
    /// # Errors
    ///
    /// Throws when `url` cannot be split into scheme, host, and path.
    #[wasm_bindgen]
    pub fn decide(&self, app_id: &str, url: &str) -> Result<String, JsError> {
        let decision = self
            .inner
            .decide(&self.domain, app_id, url)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(decision_name(decision).to_owned())
    }

    /// Decides for many URLs in one crossing of the JavaScript/WebAssembly boundary.
    ///
    /// Matching itself takes nanoseconds; marshalling a string across the boundary takes hundreds
    /// of them. Called one URL at a time, this module is no faster than a plain JavaScript
    /// implementation because the boundary, not the matcher, is the cost. Batch and it is.
    ///
    /// Returns one of `"match"`, `"exclude"`, or `"no_match"` per input, in order. A URL that
    /// cannot be split yields `"invalid_url"` rather than throwing, so one bad entry does not
    /// discard the rest of the batch.
    #[wasm_bindgen(js_name = decideMany)]
    #[must_use]
    // wasm-bindgen marshals a JavaScript `string[]` into an owned `Vec<String>`; a slice
    // is not an option here.
    #[allow(clippy::needless_pass_by_value)]
    pub fn decide_many(&self, app_id: &str, urls: Vec<String>) -> Vec<String> {
        urls.iter()
            .map(|url| {
                self.inner
                    .decide(&self.domain, app_id, url)
                    .map_or("invalid_url", decision_name)
                    .to_owned()
            })
            .collect()
    }

    /// The same batch decision, as one byte per URL: 0 no match, 1 match, 2 exclude, 3 invalid URL.
    ///
    /// The cheapest form this module offers — a single `Uint8Array` comes back rather than an
    /// array of JavaScript strings.
    #[wasm_bindgen(js_name = decideManyCodes)]
    #[must_use]
    // wasm-bindgen marshals a JavaScript `string[]` into an owned `Vec<String>`; a slice
    // is not an option here.
    #[allow(clippy::needless_pass_by_value)]
    pub fn decide_many_codes(&self, app_id: &str, urls: Vec<String>) -> Vec<u8> {
        urls.iter()
            .map(|url| match self.inner.decide(&self.domain, app_id, url) {
                Ok(MatchDecision::NoMatch) => 0,
                Ok(MatchDecision::Match) => 1,
                Ok(MatchDecision::Exclude) => 2,
                Err(_) => 3,
            })
            .collect()
    }

    /// Decides for many URLs handed over as one newline-separated string.
    ///
    /// This is the fastest shape this module offers. `decideMany` still pays a separate string
    /// encode per URL — those do not amortise, because the cost is per string rather than per
    /// call. One joined string is encoded once.
    ///
    /// Returns one byte per line: 0 no match, 1 match, 2 exclude, 3 the line was not a usable URL.
    /// Empty lines are skipped and produce no byte.
    #[wasm_bindgen(js_name = decideLines)]
    #[must_use]
    pub fn decide_lines(&self, app_id: &str, urls: &str) -> Vec<u8> {
        urls.lines()
            .filter(|line| !line.trim().is_empty())
            .map(
                |url| match self.inner.decide(&self.domain, app_id, url.trim()) {
                    Ok(MatchDecision::NoMatch) => 0,
                    Ok(MatchDecision::Match) => 1,
                    Ok(MatchDecision::Exclude) => 2,
                    Err(_) => 3,
                },
            )
            .collect()
    }

    /// The decision plus the full trace, as a JavaScript object.
    ///
    /// # Errors
    ///
    /// Throws when `url` cannot be split, or the result cannot be converted.
    #[wasm_bindgen(js_name = match)]
    pub fn match_url(&self, app_id: &str, url: &str) -> Result<JsValue, JsError> {
        let result = self
            .inner
            .match_url(&self.domain, app_id, url)
            .map_err(|error| JsError::new(&error.to_string()))?;
        to_js(&result)
    }

    /// The same result as a JSON string, for callers who would rather run `JSON.parse` themselves.
    ///
    /// Which of the two is faster depends on the engine and the size of the trace; measure before
    /// choosing.
    ///
    /// # Errors
    ///
    /// Throws when `url` cannot be split.
    #[wasm_bindgen(js_name = matchJson)]
    pub fn match_json(&self, app_id: &str, url: &str) -> Result<String, JsError> {
        let result = self
            .inner
            .match_url(&self.domain, app_id, url)
            .map_err(|error| JsError::new(&error.to_string()))?;
        blazingly_json::to_string(&result).map_err(|error| JsError::new(&error.to_string()))
    }

    /// A human-readable explanation of the decision.
    ///
    /// # Errors
    ///
    /// Throws when `url` cannot be split.
    #[wasm_bindgen]
    pub fn explain(&self, app_id: &str, url: &str) -> Result<String, JsError> {
        let result = self
            .inner
            .match_url(&self.domain, app_id, url)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(result.to_string())
    }

    /// Every app this document lets open `url`, as `[{ appId, decision }]` in document order.
    ///
    /// The inverse of `decide`: instead of asking about one app, ask which apps a URL reaches.
    /// Apps that do not match are omitted.
    ///
    /// # Errors
    ///
    /// Throws when `url` cannot be split, or the result cannot be converted.
    #[wasm_bindgen(js_name = appsForUrl)]
    pub fn apps_for_url(&self, url: &str) -> Result<JsValue, JsError> {
        let apps = self
            .inner
            .apps_for_url(&self.domain, url)
            .map_err(|error| JsError::new(&error.to_string()))?;
        let rendered: Vec<AppDecision> = apps
            .into_iter()
            .map(|(app_id, decision)| AppDecision {
                app_id,
                decision: decision_name(decision),
            })
            .collect();
        to_js(&rendered)
    }

    /// Every service this domain grants the app built from a team prefix and bundle identifier.
    ///
    /// # Errors
    ///
    /// Throws if the list cannot be converted to a JavaScript value.
    #[wasm_bindgen(js_name = servicesForBundle)]
    pub fn services_for_bundle(&self, team_id: &str, bundle_id: &str) -> Result<JsValue, JsError> {
        to_js(&self.inner.services_for_bundle(team_id, bundle_id))
    }

    /// Every application identifier with this bundle identifier, whatever its team prefix.
    ///
    /// # Errors
    ///
    /// Throws if the list cannot be converted to a JavaScript value.
    #[wasm_bindgen(js_name = appIdsForBundle)]
    pub fn app_ids_for_bundle(&self, bundle_id: &str) -> Result<JsValue, JsError> {
        to_js(&self.inner.app_ids_for_bundle(bundle_id))
    }

    /// The services this domain grants `app_id`, as an array of strings.
    ///
    /// # Errors
    ///
    /// Throws if the list cannot be converted to a JavaScript value.
    #[wasm_bindgen(js_name = servicesForApp)]
    pub fn services_for_app(&self, app_id: &str) -> Result<JsValue, JsError> {
        to_js(&self.inner.services_for_app(app_id))
    }

    /// Every application identifier under `applinks.details`.
    ///
    /// # Errors
    ///
    /// Throws if the list cannot be converted to a JavaScript value.
    #[wasm_bindgen(js_name = applinkApps)]
    pub fn applink_apps(&self) -> Result<JsValue, JsError> {
        to_js(&self.inner.applink_apps())
    }

    /// The canonical rendering, with every default resolved.
    #[wasm_bindgen(js_name = normalizedJson)]
    #[must_use]
    pub fn normalized_json(&self) -> String {
        self.inner.to_normalized_json()
    }

    /// Compares this document with another, reporting only behavioural differences.
    ///
    /// # Errors
    ///
    /// Throws if the diff cannot be converted to a JavaScript value.
    #[wasm_bindgen(js_name = semanticDiff)]
    pub fn semantic_diff(&self, other: &Aasa) -> Result<JsValue, JsError> {
        to_js(self.inner.semantic_diff(&other.inner).changes())
    }

    /// Whether two documents make the same decisions for every app.
    #[wasm_bindgen(js_name = semanticEqual)]
    #[must_use]
    pub fn semantic_equal(&self, other: &Aasa) -> bool {
        self.inner.semantic_equal(&other.inner)
    }
}

/// One entry of `appsForUrl`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppDecision {
    app_id: String,
    decision: &'static str,
}

fn decision_name(decision: MatchDecision) -> &'static str {
    match decision {
        MatchDecision::Match => "match",
        MatchDecision::Exclude => "exclude",
        MatchDecision::NoMatch => "no_match",
    }
}

/// Validates a payload without keeping a handle.
///
/// # Errors
///
/// Throws for an unusable payload.
#[wasm_bindgen(js_name = validateAasa)]
pub fn validate_aasa(bytes: &[u8]) -> Result<JsValue, JsError> {
    let report =
        blazingly_aasa::validate(bytes).map_err(|error| JsError::new(&error.to_string()))?;
    to_js(report.diagnostics())
}

/// Matches a single URL without keeping a handle.
///
/// Reparses and recompiles on every call; prefer [`Aasa::compile`] for more than one URL.
///
/// # Errors
///
/// Throws for an unusable payload or URL.
#[wasm_bindgen(js_name = matchAasa)]
pub fn match_aasa(bytes: &[u8], domain: &str, app_id: &str, url: &str) -> Result<JsValue, JsError> {
    let result = blazingly_aasa::match_url(bytes, domain, app_id, url)
        .map_err(|error| JsError::new(&error.to_string()))?;
    to_js(&result)
}

/// Compares two payloads without keeping handles.
///
/// # Errors
///
/// Throws if either payload is unusable.
#[wasm_bindgen(js_name = diffAasa)]
pub fn diff_aasa(left: &[u8], right: &[u8]) -> Result<JsValue, JsError> {
    let diff =
        blazingly_aasa::diff(left, right).map_err(|error| JsError::new(&error.to_string()))?;
    to_js(diff.changes())
}

/// Checks a single Apple wildcard pattern against a string.
///
/// # Errors
///
/// Throws when the pattern has an unterminated `$(` or an unknown variable.
#[wasm_bindgen(js_name = matchPattern)]
pub fn match_pattern(pattern: &str, input: &str, case_sensitive: bool) -> Result<bool, JsError> {
    let compiled = blazingly_aasa::WildcardPattern::compile(pattern, case_sensitive)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(compiled.matches(input))
}

/// Splits `ABCDE12345.com.example.app` into `[prefix, bundleId]`, or returns `null`.
#[wasm_bindgen(js_name = splitAppId)]
#[must_use]
pub fn split_app_id(app_id: &str) -> Option<Vec<String>> {
    blazingly_aasa::split_app_id(app_id)
        .map(|(prefix, bundle)| vec![prefix.to_owned(), bundle.to_owned()])
}

/// The Foundation release the `$(region)` and `$(lang)` tables were generated from.
#[wasm_bindgen(js_name = isoTableSource)]
#[must_use]
pub fn iso_table_source() -> String {
    blazingly_aasa::ISO_TABLE_SOURCE.to_owned()
}
