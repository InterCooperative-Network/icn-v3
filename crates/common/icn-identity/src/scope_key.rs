//! Organizational scope key used across runtime economics & reputation.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// Key used for indexing mana/reputation pools based on organizational scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScopeKey {
    Federation(String),
    Cooperative(String),
    Community(String),
    Individual(String),
}
