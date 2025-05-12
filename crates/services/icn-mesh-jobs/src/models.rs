/// Component of a bid score for explanation purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreComponent {
    /// Name of the component (e.g., "price", "reputation", "timeliness", "resources")
    pub name: String,
    
    /// Value of the component after applying weight
    pub value: f64,
    
    /// Weight used for this component
    pub weight: f64,
}

/// Summary of reputation factors for a bidder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationSummary {
    /// Overall reputation score (0-100)
    pub score: f64,
    
    /// Total number of jobs completed
    pub jobs_count: u64,
    
    /// Ratio of on-time jobs to total successful jobs
    pub on_time_ratio: f64,
}

/// Detailed explanation of a bid's score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidExplanation {
    /// Bid identifier
    pub bid_id: Option<i64>,
    
    /// DID of the bidder
    pub node_did: String,
    
    /// Total calculated score
    pub total_score: f64,
    
    /// Individual score components
    pub components: Vec<ScoreComponent>,
    
    /// Summary of reputation factors
    pub reputation_summary: ReputationSummary,
}

/// Response for bid listing with explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidsExplainResponse {
    /// The bids
    pub bids: Vec<Bid>,
    
    /// Explanations for each bid score
    pub explanations: Vec<BidExplanation>,
    
    /// Configuration used for scoring
    pub config: BidEvaluatorConfig,
}

/// Configuration for bid evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidEvaluatorConfig {
    pub weight_price: f64,
    pub weight_resources: f64,
    pub weight_reputation: f64,
    pub weight_timeliness: f64,
} 