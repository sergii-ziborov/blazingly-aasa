//! Semantic checks that run over the compiled document.
//!
//! These are the findings you cannot get from JSON schema validation: a detail entry that names no
//! app, a rule that can never be reached, a legacy file that quietly mixes two formats.

use crate::compile::CompiledAasa;
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use std::collections::BTreeMap;

pub(crate) fn semantic(compiled: &CompiledAasa) -> Vec<Diagnostic> {
    let document = &compiled.document;
    let mut diagnostics = Vec::new();

    if document.applinks.is_none()
        && document.webcredentials.is_none()
        && document.appclips.is_none()
        && document.activitycontinuation.is_none()
    {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::NoRecognizedService,
                "",
                "the document declares none of applinks, webcredentials, appclips, or \
                 activitycontinuation",
            )
            .with_help("this file grants no app anything"),
        );
    }

    check_service_apps(compiled, &mut diagnostics);

    let Some(applinks) = &document.applinks else {
        return diagnostics;
    };

    if let Some(apps) = &applinks.apps {
        if !apps.is_empty() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::LegacyAppsKeyNonEmpty,
                    "applinks.apps",
                    format!(
                        "the legacy `apps` array holds {} entries; Apple requires it to be empty",
                        apps.len()
                    ),
                )
                .with_help("set it to [] or remove it; app identifiers belong in details"),
            );
        }
    }

    if applinks.details.is_empty() {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::NoDetails,
                "applinks.details",
                "applinks declares no details, so no app can open universal links here",
            )
            .with_help("add a details entry with appIDs and components"),
        );
    }

    check_details(applinks, &mut diagnostics);
    check_rules(compiled, &mut diagnostics);
    diagnostics
}

fn check_details(applinks: &crate::model::AppLinks, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen_apps: BTreeMap<&str, usize> = BTreeMap::new();

    for (index, detail) in applinks.details.iter().enumerate() {
        let path = format!("applinks.details[{index}]");
        let declared = detail.declared_app_ids();

        if declared.is_empty() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::DetailMissingAppId,
                    &path,
                    "this entry names no application identifier",
                )
                .with_help("add `appID` or `appIDs`"),
            );
        }
        if detail.app_id.is_some() && detail.app_ids.is_some() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::DetailHasBothAppIdForms,
                    &path,
                    "this entry sets both `appID` and `appIDs`",
                )
                .with_help("keep `appIDs` and drop `appID`; this crate honours the union of both"),
            );
        }
        if detail.components.is_some() && detail.paths.is_some() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::MixedComponentsAndPaths,
                    &path,
                    "this entry uses both modern `components` and legacy `paths`",
                )
                .with_help(
                    "Apple recommends against mixing formats because the combined behaviour is \
                     unspecified; this crate evaluates components first, then paths",
                ),
            );
        }

        for app_id in declared {
            if app_id.trim().is_empty() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::EmptyAppIdentifier,
                    &path,
                    "an application identifier is empty",
                ));
                continue;
            }
            if !app_id.contains('.') {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::SuspiciousAppIdentifier,
                        &path,
                        format!("`{app_id}` has no `.` separator"),
                    )
                    .with_help(
                        "identifiers look like <Application Identifier Prefix>.<Bundle Identifier>",
                    ),
                );
            }
            if let Some(first) = seen_apps.get(app_id) {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::DuplicateAppIdentifier,
                        &path,
                        format!("`{app_id}` already appears in applinks.details[{first}]"),
                    )
                    .with_help(
                        "Apple does not document how multiple entries for one app interact; this \
                         crate evaluates them in order",
                    ),
                );
            } else {
                seen_apps.insert(app_id, index);
            }
        }
    }
}

fn check_service_apps(compiled: &CompiledAasa, diagnostics: &mut Vec<Diagnostic>) {
    for (service, apps) in [
        ("webcredentials", &compiled.webcredentials),
        ("appclips", &compiled.appclips),
        ("activitycontinuation", &compiled.activitycontinuation),
    ] {
        let mut seen: Vec<&str> = Vec::new();
        for (index, app) in apps.iter().enumerate() {
            let path = format!("{service}.apps[{index}]");
            if app.trim().is_empty() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::EmptyAppIdentifier,
                    &path,
                    "an application identifier is empty",
                ));
                continue;
            }
            if seen.contains(&app.as_str()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::DuplicateAppIdentifier,
                    &path,
                    format!("`{app}` is listed more than once under {service}"),
                ));
            } else {
                seen.push(app);
            }
            if !app.contains('.') {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::SuspiciousAppIdentifier,
                    &path,
                    format!("`{app}` has no `.` separator"),
                ));
            }
        }
    }
}

fn check_rules(compiled: &CompiledAasa, diagnostics: &mut Vec<Diagnostic>) {
    for detail in &compiled.details {
        let mut catch_all: Option<usize> = None;
        for rule in &detail.rules {
            let path = rule_path(detail.index, rule.rule_index, rule.legacy);

            if rule.is_unconstrained() {
                if !rule.legacy {
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::EmptyComponentRule,
                            &path,
                            "this rule constrains no URL component, so it matches every URL",
                        )
                        .with_help(if rule.exclude {
                            "it blocks the whole domain for this app"
                        } else {
                            "it opens the whole domain for this app; add `/`, `?`, or `#` if that \
                             was not intended"
                        }),
                    );
                }
                if catch_all.is_none() {
                    catch_all = Some(rule.rule_index);
                }
                continue;
            }

            if let Some(first) = catch_all {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::UnreachableRule,
                        &path,
                        format!("rule #{first} already matches every URL, so this rule never runs"),
                    )
                    .with_help("the first matching rule wins; move this rule above the catch-all"),
                );
            }
        }
    }
}

fn rule_path(detail_index: usize, rule_index: usize, legacy: bool) -> String {
    if legacy {
        format!("applinks.details[{detail_index}].paths[{rule_index}]")
    } else {
        format!("applinks.details[{detail_index}].components[{rule_index}]")
    }
}
