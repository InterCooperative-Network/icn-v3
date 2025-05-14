use super::TokenAmount;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPolicy {
    /// Weight of reputation in scoring (0.0 to 1.0).
    pub rep_weight: f64,
    /// Weight of bid price in scoring (0.0 to 1.0).
    pub price_weight: f64,
    /// Optional region constraint for executors.
    pub region_filter: Option<String>,
    /// Optional minimum required reputation to be eligible.
    pub min_reputation: Option<f64>,
    /// Optional maximum acceptable bid price (in TokenAmount).
    pub max_price: Option<TokenAmount>,
}
