//! Type compatibility primitives used by connection rules and profiles.

use super::TypeDesc;

mod default;

pub use default::DefaultTypeCompatibility;

/// Result of a type compatibility check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCompatibilityResult {
    Compatible,
    Incompatible { reason: String },
}

impl TypeCompatibilityResult {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible)
    }
}

/// Compatibility policy between two types.
///
/// The direction is `from -> to` (assignability), matching edge direction for data flow.
pub trait TypeCompatibility {
    fn compatible(&mut self, from: &TypeDesc, to: &TypeDesc) -> TypeCompatibilityResult;
}
