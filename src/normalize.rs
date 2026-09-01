//! A canonical rendering of a compiled document.
//!
//! Useful for debugging, for golden tests, and for a textual diff that is actually meaningful:
//! defaults are resolved into every rule, so two files that behave identically render identically.

use crate::compile::{CompiledAasa, EffectiveRule, Service};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct NormalizedDocument<'a> {
    /// App identifiers per service, sorted; order carries no meaning here.
    services: BTreeMap<&'static str, Vec<&'a str>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    substitution_variables: &'a BTreeMap<String, Vec<String>>,
    /// Detail entries in source order, because their order is significant.
    applinks: Vec<NormalizedDetail<'a>>,
}

#[derive(Serialize)]
struct NormalizedDetail<'a> {
    index: usize,
    app_ids: &'a [String],
    rules: Vec<EffectiveRule>,
}

impl CompiledAasa {
    /// Renders the document in a canonical form with every default resolved.
    ///
    /// Rule order is preserved exactly; app lists are sorted, since their order carries no
    /// meaning.
    ///
    /// # Panics
    ///
    /// Never in practice: the normalized model contains only maps, arrays, strings, and booleans,
    /// all of which serialize infallibly.
    #[must_use]
    pub fn to_normalized_json(&self) -> String {
        let mut services = BTreeMap::new();
        for service in [
            Service::AppLinks,
            Service::WebCredentials,
            Service::AppClips,
            Service::ActivityContinuation,
        ] {
            let apps = self.apps_for_service(service);
            if !apps.is_empty() || self.declares_service(service) {
                let mut apps = apps;
                apps.sort_unstable();
                apps.dedup();
                services.insert(service.key(), apps);
            }
        }

        let applinks = self
            .details
            .iter()
            .map(|detail| NormalizedDetail {
                index: detail.index,
                app_ids: &detail.app_ids,
                rules: detail
                    .rules
                    .iter()
                    .map(crate::compile::CompiledRule::effective_rule)
                    .collect(),
            })
            .collect();

        let normalized = NormalizedDocument {
            services,
            substitution_variables: self.substitution_variables(),
            applinks,
        };
        blazingly_json::to_string_pretty(&normalized)
            .expect("the normalized model always serializes")
    }

    fn declares_service(&self, service: Service) -> bool {
        match service {
            Service::AppLinks => self.has_applinks,
            Service::WebCredentials => self.document.webcredentials.is_some(),
            Service::AppClips => self.document.appclips.is_some(),
            Service::ActivityContinuation => self.document.activitycontinuation.is_some(),
        }
    }
}
