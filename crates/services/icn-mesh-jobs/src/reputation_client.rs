use anyhow::Result;
use icn_identity::Did;
use icn_types::reputation::{ReputationProfile, ReputationRecord}; // Assuming this path is correct based on icn-types structure
use serde::Deserialize;

// Helper struct for deserializing just the score if that's all we need initially.
// However, the plan specifies deserializing the whole ReputationProfile.
// #[derive(Deserialize, Debug)]
// struct ReputationScoreResponse {
//     computed_score: f64,
// }

/// Fetches the reputation profile for a given node DID from the reputation service
/// and returns its computed score.
pub async fn get_reputation_score(node_id: &Did, base_url: &str) -> Result<Option<f64>> {
    // Ensure base_url doesn't have a trailing slash, and construct the full URL.
    let base = base_url.trim_end_matches('/');
    let url = format!("{}/reputation/profiles/{}", base, node_id.0); // Accessing inner String of Did

    tracing::debug!("Querying reputation score for {} at URL: {}", node_id.0, url);

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;

    if resp.status().is_success() {
        // Attempt to deserialize the full ReputationProfile
        match resp.json::<ReputationProfile>().await {
            Ok(profile) => {
                tracing::debug!("Successfully fetched reputation profile for {}: score = {}", node_id.0, profile.computed_score);
                Ok(Some(profile.computed_score))
            }
            Err(e) => {
                tracing::error!("Failed to deserialize ReputationProfile for {}: {}. Response: {:?}", node_id.0, e, resp.text().await.unwrap_or_else(|_| "<failed to read body>".to_string()));
                Err(anyhow::anyhow!("Failed to deserialize reputation profile: {}", e))
            }
        }
    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::debug!("Reputation profile not found for {}", node_id.0);
        Ok(None) // Node has no reputation profile yet, or service returned 404 correctly
    } else {
        let status = resp.status();
        let error_body = resp.text().await.unwrap_or_else(|_| "<no body>".to_string());
        tracing::error!("Reputation query for {} failed with status {}: {}", node_id.0, status, error_body);
        Err(anyhow::anyhow!(
            "Reputation service query failed for node {} with status {}: {}",
            node_id.0, status, error_body
        ))
    }
}

/// Submits a reputation record to the reputation service.
pub async fn submit_reputation_record(record: &ReputationRecord, base_url: &str) -> Result<()> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{}/reputation/records", base);

    tracing::debug!("Submitting reputation record for subject {} to URL: {}", record.subject.0, url);

    let client = reqwest::Client::new();
    let resp = client.post(&url).json(record).send().await?;

    if resp.status().is_success() || resp.status() == reqwest::StatusCode::CREATED {
        tracing::info!(
            "Successfully submitted reputation record for subject {}. Status: {}",
            record.subject.0,
            resp.status()
        );
        Ok(())
    } else {
        let status = resp.status();
        let error_body = resp.text().await.unwrap_or_else(|_| "<no body>".to_string());
        tracing::error!(
            "Failed to submit reputation record for subject {}. Status: {}. Body: {}",
            record.subject.0, status, error_body
        );
        Err(anyhow::anyhow!(
            "Reputation service failed to accept record for subject {} with status {}: {}",
            record.subject.0, status, error_body
        ))
    }
} 