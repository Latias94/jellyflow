use serde::{Deserialize, Serialize};

/// Support level an adapter can claim for a conformance capability.
///
/// The ordering is intentional: a `full` adapter satisfies `partial`, while a
/// `projection` proof does not satisfy editable or layout-pass expectations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceSupportLevel {
    #[default]
    None,
    Projection,
    Partial,
    Full,
}

impl ConformanceSupportLevel {
    pub fn satisfies(self, minimum: Self) -> bool {
        self >= minimum
    }

    pub fn is_supported(self) -> bool {
        self != Self::None
    }
}

/// Renderer-neutral capability names used by adapter conformance reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceCapabilityKind {
    MeasuredHandles,
    MeasuredAnchors,
    DynamicInternals,
    ControlProjection,
    EditableControls,
    RepeatableCollections,
    Actions,
    Menus,
    DroppedWireMenu,
    Inspector,
    Blackboard,
    TypedDiagnostics,
    VisualRegression,
    KeyboardAccessibility,
    LayoutPassMeasurement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCapabilityClaim {
    pub capability: ConformanceCapabilityKind,
    pub level: ConformanceSupportLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl ConformanceCapabilityClaim {
    pub fn new(capability: ConformanceCapabilityKind, level: ConformanceSupportLevel) -> Self {
        Self {
            capability,
            level,
            notes: None,
        }
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    pub fn projection(capability: ConformanceCapabilityKind) -> Self {
        Self::new(capability, ConformanceSupportLevel::Projection)
    }

    pub fn partial(capability: ConformanceCapabilityKind) -> Self {
        Self::new(capability, ConformanceSupportLevel::Partial)
    }

    pub fn full(capability: ConformanceCapabilityKind) -> Self {
        Self::new(capability, ConformanceSupportLevel::Full)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCapabilityRequirement {
    pub capability: ConformanceCapabilityKind,
    pub minimum: ConformanceSupportLevel,
}

impl ConformanceCapabilityRequirement {
    pub fn new(capability: ConformanceCapabilityKind, minimum: ConformanceSupportLevel) -> Self {
        Self {
            capability,
            minimum,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCapabilityMatrix {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<ConformanceCapabilityClaim>,
}

impl ConformanceCapabilityMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_adapter(adapter: impl Into<String>) -> Self {
        Self {
            adapter: Some(adapter.into()),
            claims: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.adapter.is_none() && self.claims.is_empty()
    }

    pub fn with_claim(mut self, claim: ConformanceCapabilityClaim) -> Self {
        self.set_claim(claim);
        self
    }

    pub fn set_claim(&mut self, claim: ConformanceCapabilityClaim) {
        if let Some(existing) = self
            .claims
            .iter_mut()
            .find(|existing| existing.capability == claim.capability)
        {
            *existing = claim;
        } else {
            self.claims.push(claim);
        }
    }

    pub fn claim(
        &self,
        capability: ConformanceCapabilityKind,
    ) -> Option<&ConformanceCapabilityClaim> {
        self.claims
            .iter()
            .find(|claim| claim.capability == capability)
    }

    pub fn level(&self, capability: ConformanceCapabilityKind) -> ConformanceSupportLevel {
        self.claim(capability)
            .map(|claim| claim.level)
            .unwrap_or_default()
    }

    pub fn satisfies(
        &self,
        capability: ConformanceCapabilityKind,
        minimum: ConformanceSupportLevel,
    ) -> bool {
        self.level(capability).satisfies(minimum)
    }

    pub fn gaps<'a>(
        &self,
        requirements: impl IntoIterator<Item = &'a ConformanceCapabilityRequirement>,
    ) -> Vec<ConformanceCapabilityGap> {
        requirements
            .into_iter()
            .filter_map(|requirement| {
                let actual = self.level(requirement.capability);
                (!actual.satisfies(requirement.minimum)).then_some(ConformanceCapabilityGap {
                    capability: requirement.capability,
                    required: requirement.minimum,
                    actual,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceCapabilityGap {
    pub capability: ConformanceCapabilityKind,
    pub required: ConformanceSupportLevel,
    pub actual: ConformanceSupportLevel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_levels_are_ordered_by_strength() {
        assert!(ConformanceSupportLevel::Full.satisfies(ConformanceSupportLevel::Partial));
        assert!(ConformanceSupportLevel::Partial.satisfies(ConformanceSupportLevel::Projection));
        assert!(!ConformanceSupportLevel::Projection.satisfies(ConformanceSupportLevel::Partial));
    }

    #[test]
    fn matrix_reports_missing_capabilities_as_none() {
        let matrix = ConformanceCapabilityMatrix::for_adapter("unit").with_claim(
            ConformanceCapabilityClaim::partial(ConformanceCapabilityKind::ControlProjection),
        );
        let requirements = [
            ConformanceCapabilityRequirement::new(
                ConformanceCapabilityKind::ControlProjection,
                ConformanceSupportLevel::Projection,
            ),
            ConformanceCapabilityRequirement::new(
                ConformanceCapabilityKind::LayoutPassMeasurement,
                ConformanceSupportLevel::Full,
            ),
        ];

        let gaps = matrix.gaps(&requirements);

        assert_eq!(gaps.len(), 1);
        assert_eq!(
            gaps[0].capability,
            ConformanceCapabilityKind::LayoutPassMeasurement
        );
        assert_eq!(gaps[0].actual, ConformanceSupportLevel::None);
    }
}
