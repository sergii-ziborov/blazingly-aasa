//! Semantic comparison of two association files.
//!
//! A textual diff of two `apple-app-site-association` files is mostly noise: key order changes,
//! `caseSensitive` moves from a component up to `defaults`, whitespace shifts. What matters is
//! whether the *decisions* changed. This module compares the effective, order-preserving rule list
//! per app, so a pure refactor reports no changes while a reordering does.
//!
//! Equivalence is only ever claimed when normalisation can prove it. Anything this crate cannot
//! reduce to a comparable form is reported as a change.

use crate::compile::{CompiledAasa, EffectiveRule, Service};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;

/// One semantic difference between two documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticChange {
    /// A service section appeared.
    ServiceAdded {
        /// The service.
        service: Service,
    },
    /// A service section disappeared.
    ServiceRemoved {
        /// The service.
        service: Service,
    },
    /// An app gained access to a service.
    AppAdded {
        /// The service.
        service: Service,
        /// The application identifier.
        app_id: String,
    },
    /// An app lost access to a service.
    AppRemoved {
        /// The service.
        service: Service,
        /// The application identifier.
        app_id: String,
    },
    /// A rule was added for an app.
    RuleAdded {
        /// The application identifier.
        app_id: String,
        /// Position in the right-hand rule list.
        index: usize,
        /// The rule.
        rule: EffectiveRule,
    },
    /// A rule was removed for an app.
    RuleRemoved {
        /// The application identifier.
        app_id: String,
        /// Position in the left-hand rule list.
        index: usize,
        /// The rule.
        rule: EffectiveRule,
    },
    /// A rule kept its patterns but changed its settings or its `exclude` flag.
    RuleChanged {
        /// The application identifier.
        app_id: String,
        /// Position in the left-hand rule list.
        index: usize,
        /// The rule before.
        left: EffectiveRule,
        /// The rule after.
        right: EffectiveRule,
    },
    /// A rule kept its meaning but moved, which changes which rule wins first.
    RuleMoved {
        /// The application identifier.
        app_id: String,
        /// Position in the left-hand rule list.
        from: usize,
        /// Position in the right-hand rule list.
        to: usize,
        /// The rule.
        rule: EffectiveRule,
    },
    /// A substitution variable appeared.
    SubstitutionAdded {
        /// The variable name.
        name: String,
        /// Its values.
        values: Vec<String>,
    },
    /// A substitution variable disappeared.
    SubstitutionRemoved {
        /// The variable name.
        name: String,
        /// Its values.
        values: Vec<String>,
    },
    /// A substitution variable's values changed.
    SubstitutionChanged {
        /// The variable name.
        name: String,
        /// Values before.
        left: Vec<String>,
        /// Values after.
        right: Vec<String>,
    },
}

impl fmt::Display for SemanticChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ServiceAdded { service } => write!(f, "SERVICE_ADDED   {service}"),
            Self::ServiceRemoved { service } => write!(f, "SERVICE_REMOVED {service}"),
            Self::AppAdded { service, app_id } => write!(f, "APP_ADDED       {service}: {app_id}"),
            Self::AppRemoved { service, app_id } => {
                write!(f, "APP_REMOVED     {service}: {app_id}")
            }
            Self::RuleAdded {
                app_id,
                index,
                rule,
            } => {
                write!(f, "RULE_ADDED      {app_id} #{index}\n  {rule}")
            }
            Self::RuleRemoved {
                app_id,
                index,
                rule,
            } => {
                write!(f, "RULE_REMOVED    {app_id} #{index}\n  {rule}")
            }
            Self::RuleChanged {
                app_id,
                index,
                left,
                right,
            } => write!(
                f,
                "RULE_CHANGED    {app_id} #{index}\n  before: {left}\n  after:  {right}"
            ),
            Self::RuleMoved {
                app_id,
                from,
                to,
                rule,
            } => write!(f, "RULE_MOVED      {app_id} #{from} -> #{to}\n  {rule}"),
            Self::SubstitutionAdded { name, values } => {
                write!(f, "SUBST_ADDED     ${name} = {values:?}")
            }
            Self::SubstitutionRemoved { name, values } => {
                write!(f, "SUBST_REMOVED   ${name} = {values:?}")
            }
            Self::SubstitutionChanged { name, left, right } => write!(
                f,
                "SUBST_CHANGED   ${name}\n  before: {left:?}\n  after:  {right:?}"
            ),
        }
    }
}

/// The result of comparing two documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AasaDiff {
    changes: Vec<SemanticChange>,
}

impl AasaDiff {
    /// Whether the two documents make the same decisions for every app.
    #[must_use]
    pub fn is_equivalent(&self) -> bool {
        self.changes.is_empty()
    }

    /// Every difference found.
    #[must_use]
    pub fn changes(&self) -> &[SemanticChange] {
        &self.changes
    }
}

impl fmt::Display for AasaDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.changes.is_empty() {
            return f.write_str("semantically equivalent");
        }
        for (index, change) in self.changes.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{change}")?;
        }
        Ok(())
    }
}

const SERVICES: [Service; 4] = [
    Service::AppLinks,
    Service::WebCredentials,
    Service::AppClips,
    Service::ActivityContinuation,
];

impl CompiledAasa {
    /// Compares this document with `other`, reporting only differences that change behaviour.
    #[must_use]
    pub fn semantic_diff(&self, other: &Self) -> AasaDiff {
        let mut changes = Vec::new();

        for service in SERVICES {
            let left_present = self.declares(service);
            let right_present = other.declares(service);
            match (left_present, right_present) {
                (false, true) => changes.push(SemanticChange::ServiceAdded { service }),
                (true, false) => changes.push(SemanticChange::ServiceRemoved { service }),
                _ => {}
            }

            let left: BTreeSet<&str> = self.apps_for_service(service).into_iter().collect();
            let right: BTreeSet<&str> = other.apps_for_service(service).into_iter().collect();
            for app_id in right.difference(&left) {
                changes.push(SemanticChange::AppAdded {
                    service,
                    app_id: (*app_id).to_owned(),
                });
            }
            for app_id in left.difference(&right) {
                changes.push(SemanticChange::AppRemoved {
                    service,
                    app_id: (*app_id).to_owned(),
                });
            }
        }

        let apps: BTreeSet<&str> = self
            .applink_apps()
            .into_iter()
            .chain(other.applink_apps())
            .collect();
        for app_id in apps {
            diff_rules(
                app_id,
                &self.effective_rules_for(app_id),
                &other.effective_rules_for(app_id),
                &mut changes,
            );
        }

        diff_substitutions(self, other, &mut changes);
        AasaDiff { changes }
    }

    /// Whether the two documents make the same decisions for every app.
    #[must_use]
    pub fn semantic_equal(&self, other: &Self) -> bool {
        self.semantic_diff(other).is_equivalent()
    }

    /// Whether the two wire models are identical, key for key.
    #[must_use]
    pub fn structural_equal(&self, other: &Self) -> bool {
        self.document == other.document
    }

    fn declares(&self, service: Service) -> bool {
        match service {
            Service::AppLinks => self.has_applinks,
            Service::WebCredentials => self.document.webcredentials.is_some(),
            Service::AppClips => self.document.appclips.is_some(),
            Service::ActivityContinuation => self.document.activitycontinuation.is_some(),
        }
    }
}

fn diff_substitutions(
    left: &CompiledAasa,
    right: &CompiledAasa,
    changes: &mut Vec<SemanticChange>,
) {
    let names: BTreeSet<&String> = left
        .substitution_variables()
        .keys()
        .chain(right.substitution_variables().keys())
        .collect();
    for name in names {
        match (
            left.substitution_variables().get(name),
            right.substitution_variables().get(name),
        ) {
            (None, Some(values)) => changes.push(SemanticChange::SubstitutionAdded {
                name: name.clone(),
                values: values.clone(),
            }),
            (Some(values), None) => changes.push(SemanticChange::SubstitutionRemoved {
                name: name.clone(),
                values: values.clone(),
            }),
            (Some(before), Some(after)) if before != after => {
                changes.push(SemanticChange::SubstitutionChanged {
                    name: name.clone(),
                    left: before.clone(),
                    right: after.clone(),
                });
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Edit {
    Keep,
    Remove(usize),
    Add(usize),
}

fn diff_rules(
    app_id: &str,
    left: &[EffectiveRule],
    right: &[EffectiveRule],
    changes: &mut Vec<SemanticChange>,
) {
    if left == right {
        return;
    }
    let edits = edit_script(left, right);

    let mut removed: Vec<usize> = Vec::new();
    let mut added: Vec<usize> = Vec::new();
    for edit in edits {
        match edit {
            Edit::Keep => {}
            Edit::Remove(index) => removed.push(index),
            Edit::Add(index) => added.push(index),
        }
    }

    let mut added_taken = vec![false; added.len()];
    let mut removed_taken = vec![false; removed.len()];

    // A removal and an insertion of the very same rule is a reorder, not a rewrite.
    for (removed_slot, left_index) in removed.iter().enumerate() {
        for (added_slot, right_index) in added.iter().enumerate() {
            if added_taken[added_slot] {
                continue;
            }
            if left[*left_index] == right[*right_index] {
                removed_taken[removed_slot] = true;
                added_taken[added_slot] = true;
                changes.push(SemanticChange::RuleMoved {
                    app_id: app_id.to_owned(),
                    from: *left_index,
                    to: *right_index,
                    rule: left[*left_index].clone(),
                });
                break;
            }
        }
    }

    // Same patterns, different settings: report it as one change rather than a delete plus an add.
    for (removed_slot, left_index) in removed.iter().enumerate() {
        if removed_taken[removed_slot] {
            continue;
        }
        for (added_slot, right_index) in added.iter().enumerate() {
            if added_taken[added_slot] {
                continue;
            }
            if same_shape(&left[*left_index], &right[*right_index]) {
                removed_taken[removed_slot] = true;
                added_taken[added_slot] = true;
                changes.push(SemanticChange::RuleChanged {
                    app_id: app_id.to_owned(),
                    index: *left_index,
                    left: left[*left_index].clone(),
                    right: right[*right_index].clone(),
                });
                break;
            }
        }
    }

    for (slot, left_index) in removed.iter().enumerate() {
        if !removed_taken[slot] {
            changes.push(SemanticChange::RuleRemoved {
                app_id: app_id.to_owned(),
                index: *left_index,
                rule: left[*left_index].clone(),
            });
        }
    }
    for (slot, right_index) in added.iter().enumerate() {
        if !added_taken[slot] {
            changes.push(SemanticChange::RuleAdded {
                app_id: app_id.to_owned(),
                index: *right_index,
                rule: right[*right_index].clone(),
            });
        }
    }
}

fn same_shape(left: &EffectiveRule, right: &EffectiveRule) -> bool {
    left.path == right.path && left.query == right.query && left.fragment == right.fragment
}

/// A longest-common-subsequence edit script, so an insertion does not shift every later rule.
fn edit_script(left: &[EffectiveRule], right: &[EffectiveRule]) -> Vec<Edit> {
    const MAX_CELLS: usize = 1 << 20;
    if left.len().saturating_mul(right.len()) > MAX_CELLS {
        // Degenerate input: fall back to a positional comparison rather than allocating a huge
        // table. Real association files never reach this.
        let mut edits = Vec::new();
        for index in 0..left.len().max(right.len()) {
            match (left.get(index), right.get(index)) {
                (Some(a), Some(b)) if a == b => edits.push(Edit::Keep),
                (Some(_), Some(_)) => {
                    edits.push(Edit::Remove(index));
                    edits.push(Edit::Add(index));
                }
                (Some(_), None) => edits.push(Edit::Remove(index)),
                (None, Some(_)) => edits.push(Edit::Add(index)),
                (None, None) => {}
            }
        }
        return edits;
    }

    let rows = left.len() + 1;
    let columns = right.len() + 1;
    let mut table = vec![0usize; rows * columns];
    for row in (0..left.len()).rev() {
        for column in (0..right.len()).rev() {
            table[row * columns + column] = if left[row] == right[column] {
                table[(row + 1) * columns + column + 1] + 1
            } else {
                table[(row + 1) * columns + column].max(table[row * columns + column + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut row, mut column) = (0usize, 0usize);
    while row < left.len() && column < right.len() {
        if left[row] == right[column] {
            edits.push(Edit::Keep);
            row += 1;
            column += 1;
        } else if table[(row + 1) * columns + column] >= table[row * columns + column + 1] {
            edits.push(Edit::Remove(row));
            row += 1;
        } else {
            edits.push(Edit::Add(column));
            column += 1;
        }
    }
    while row < left.len() {
        edits.push(Edit::Remove(row));
        row += 1;
    }
    while column < right.len() {
        edits.push(Edit::Add(column));
        column += 1;
    }
    edits
}
