// Integration tests for AgoraNet API

// TODO: Add tests for the following flow:
// 1. Create a new thread
// 2. Create a new proposal linked to that thread
// 3. Cast a few votes on the proposal (approve, reject, abstain)
// 4. Get the thread detail to see if proposal is listed (optional, if API supports)
// 5. Get the proposal detail to see vote counts
// 6. Get the proposal votes to see individual votes and summary

// Example test (needs reqwest or similar HTTP client)
/*
#[tokio::test]
async fn dummy_test() {
    assert_eq!(2 + 2, 4);
}
*/

// Note: To run these tests, the icn-agoranet server needs to be running.
// We might need to add a helper to spawn the server process for testing
// or use a library like `axum-test-helper` or `hyper` directly to make requests
// without running a full server.

// use axum::{ // Commened out entire block
//     // body::Body, 
//     // http::{Request, StatusCode}, 
//     // Router, 
// };
use chrono::{Duration, Utc};
use icn_agoranet::{
    handlers::{
        // cast_vote_handler, create_proposal_handler, create_thread_handler,
        // get_proposal_detail_handler, get_proposal_votes_handler, get_threads_handler,
        // health_check_handler, 
        Db,
        // InMemoryStore,
    },
    models::{
        // GetProposalsQuery, GetThreadsQuery, Message,
        NewProposalRequest, NewThreadRequest, NewVoteRequest,
        ProposalDetail, ProposalStatus, ProposalSummary,
        ProposalVotesResponse, ThreadDetail, ThreadSummary,
        // Timestamp,
        Vote, VoteCounts, VoteType,
    },
};
use reqwest::Client;
use serde_json::json; // For ad-hoc json creation in tests
// use std::net::SocketAddr;
// use std::sync::{Arc, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;
use tokio::net::TcpListener;
// use tower::ServiceExt;

use icn_agoranet::app::create_app;

// const BASE_URL: &str = "http://127.0.0.1:8787"; // This line will be removed

// Helper to create a new thread
async fn create_thread(
    client: &Client,
    base_url_for_test: &str,
    title: &str,
    author_did: &str,
    scope: &str,
) -> ThreadSummary {
    let req = NewThreadRequest {
        title: title.to_string(),
        author_did: author_did.to_string(),
        scope: scope.to_string(),
        metadata: Some(json!({"test_metadata": "some_value"})),
    };
    client
        .post(format!("{}/threads", base_url_for_test))
        .json(&req)
        .send()
        .await
        .expect("Failed to send create thread request")
        .json::<ThreadSummary>()
        .await
        .expect("Failed to parse create thread response")
}

// Helper to create a new proposal
async fn create_proposal(
    client: &Client,
    base_url_for_test: &str,
    title: &str,
    full_text: &str,
    scope: &str,
    thread_id: Option<String>,
) -> ProposalDetail {
    // Assuming create returns ProposalDetail for easier access to ID
    let req = NewProposalRequest {
        title: title.to_string(),
        full_text: full_text.to_string(),
        scope: scope.to_string(),
        linked_thread_id: thread_id,
        voting_deadline: Some(Utc::now() + Duration::days(7)),
    };
    // The API actually returns ProposalSummary, but we fetch ProposalDetail immediately
    let summary = client
        .post(format!("{}/proposals", base_url_for_test))
        .json(&req)
        .send()
        .await
        .expect("Failed to send create proposal request")
        .json::<icn_agoranet::models::ProposalSummary>()
        .await
        .expect("Failed to parse create proposal response");

    // Fetch the ProposalDetail to get all fields, including the ID.
    get_proposal_detail(client, base_url_for_test, &summary.id).await
}

// Helper to get proposal detail
async fn get_proposal_detail(client: &Client, base_url_for_test: &str, proposal_id: &str) -> ProposalDetail {
    client
        .get(format!("{}/proposals/{}", base_url_for_test, proposal_id))
        .send()
        .await
        .expect("Failed to send get proposal detail request")
        .json::<ProposalDetail>()
        .await
        .expect("Failed to parse get proposal detail response")
}

// Helper to cast a vote
async fn cast_vote(
    client: &Client,
    base_url_for_test: &str,
    proposal_id: &str,
    voter_did: &str,
    vote_type: VoteType,
    justification: Option<String>,
) -> Vote {
    let req = NewVoteRequest {
        proposal_id: proposal_id.to_string(),
        voter_did: voter_did.to_string(),
        vote_type,
        justification,
    };
    client
        .post(format!("{}/votes", base_url_for_test))
        .json(&req)
        .send()
        .await
        .expect("Failed to send cast vote request")
        .json::<Vote>()
        .await
        .expect("Failed to parse cast vote response")
}

// Helper to get proposal votes
async fn get_proposal_votes(client: &Client, base_url_for_test: &str, proposal_id: &str) -> ProposalVotesResponse {
    client
        .get(format!("{}/votes/{}", base_url_for_test, proposal_id))
        .send()
        .await
        .expect("Failed to send get proposal votes request")
        .json::<ProposalVotesResponse>()
        .await
        .expect("Failed to parse get proposal votes response")
}

// Helper to get thread detail
async fn get_thread_detail(client: &Client, base_url_for_test: &str, thread_id: &str) -> ThreadDetail {
    client
        .get(format!("{}/threads/{}", base_url_for_test, thread_id))
        .send()
        .await
        .expect("Failed to send get thread detail request")
        .json::<ThreadDetail>()
        .await
        .expect("Failed to parse get thread detail response")
}

// Helper function to spawn the app in the background
async fn spawn_app() -> (String, JoinHandle<()>, Db) {
    let store = Db::default(); // Or InMemoryStore::new() if that's the constructor
    let app = create_app(store.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap(); // Bind to a random available port
    let local_addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", local_addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (server_url, handle, store)
}

#[tokio::test]
async fn test_create_proposal_handler() {
    let (server_url, _handle, _db) = spawn_app().await;
    let client = reqwest::Client::new();

    let thread_id = format!("thread_{}", Uuid::new_v4()); // Example thread_id

    let response = client
        .post(format!("{}/proposals", server_url))
        .json(&NewProposalRequest {
            title: "Test Proposal from Integration Test".to_string(),
            full_text: "This is a test proposal.".to_string(),
            scope: "test.scope".to_string(),
            linked_thread_id: Some(thread_id.clone()),
            voting_deadline: Some(Utc::now() + Duration::days(7)),
        })
        .send()
        .await
        .expect("Failed to create proposal");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
}

#[tokio::test]
async fn test_full_lifecycle() {
    let (server_url, _handle, _db) = spawn_app().await;
    let client = Client::new();

    // 1. Create a new thread
    let thread_title = "Integration Test Thread";
    let thread_author = "did:test:thread_author";
    let thread_scope = "test.scope.thread";
    let created_thread_summary =
        create_thread(&client, &server_url, thread_title, thread_author, thread_scope).await;

    assert_eq!(created_thread_summary.title, thread_title);
    assert_eq!(created_thread_summary.author_did, thread_author);
    assert_eq!(created_thread_summary.scope, thread_scope);
    println!("Created thread: {}", created_thread_summary.id);

    // Fetch thread detail to verify
    let thread_detail = get_thread_detail(&client, &server_url, &created_thread_summary.id).await;
    assert_eq!(thread_detail.summary.id, created_thread_summary.id);
    assert_eq!(thread_detail.messages.len(), 0); // New threads have no messages initially (as per current model)

    // 2. Create a new proposal linked to that thread
    let proposal_title = "Integration Test Proposal";
    let proposal_text = "This is a detailed proposal for integration testing.";
    let proposal_scope = "test.scope.proposal";
    let created_proposal_detail = create_proposal(
        &client,
        &server_url,
        proposal_title,
        proposal_text,
        proposal_scope,
        Some(created_thread_summary.id.clone()),
    )
    .await;

    assert_eq!(created_proposal_detail.summary.title, proposal_title);
    assert_eq!(created_proposal_detail.full_text, proposal_text);
    assert_eq!(created_proposal_detail.summary.scope, proposal_scope);
    assert_eq!(
        created_proposal_detail.linked_thread_id,
        Some(created_thread_summary.id.clone())
    );
    assert_eq!(created_proposal_detail.summary.status, ProposalStatus::Open);
    assert_eq!(created_proposal_detail.summary.vote_counts.approve, 0);
    println!("Created proposal: {}", created_proposal_detail.summary.id);

    // 3. Cast a few votes on the proposal
    let voter1 = "did:test:voter1";
    let voter2 = "did:test:voter2";
    let voter3 = "did:test:voter3";

    let vote1 = cast_vote(
        &client,
        &server_url,
        &created_proposal_detail.summary.id,
        voter1,
        VoteType::Approve,
        Some("Looks good to me!".to_string()),
    )
    .await;
    assert_eq!(vote1.voter_did, voter1);
    assert_eq!(vote1.vote_type, VoteType::Approve);
    println!("Casted vote 1: {:?}", vote1);

    let vote2 = cast_vote(
        &client,
        &server_url,
        &created_proposal_detail.summary.id,
        voter2,
        VoteType::Reject,
        None,
    )
    .await;
    assert_eq!(vote2.voter_did, voter2);
    assert_eq!(vote2.vote_type, VoteType::Reject);
    println!("Casted vote 2: {:?}", vote2);

    let vote3 = cast_vote(
        &client,
        &server_url,
        &created_proposal_detail.summary.id,
        voter3,
        VoteType::Abstain,
        Some("Need more info.".to_string()),
    )
    .await;
    assert_eq!(vote3.voter_did, voter3);
    assert_eq!(vote3.vote_type, VoteType::Abstain);
    println!("Casted vote 3: {:?}", vote3);

    // 4. Get the proposal detail to see updated vote counts
    let updated_proposal_detail =
        get_proposal_detail(&client, &server_url, &created_proposal_detail.summary.id).await;
    assert_eq!(updated_proposal_detail.summary.vote_counts.approve, 1);
    assert_eq!(updated_proposal_detail.summary.vote_counts.reject, 1);
    assert_eq!(updated_proposal_detail.summary.vote_counts.abstain, 1);
    println!(
        "Updated proposal detail vote counts: {:?}",
        updated_proposal_detail.summary.vote_counts
    );

    // 5. Get the proposal votes to see individual votes and summary
    let proposal_votes_response =
        get_proposal_votes(&client, &created_proposal_detail.summary.id).await;
    assert_eq!(proposal_votes_response.votes.len(), 3);
    assert!(proposal_votes_response
        .votes
        .iter()
        .any(|v| v.voter_did == voter1 && v.vote_type == VoteType::Approve));
    assert!(proposal_votes_response
        .votes
        .iter()
        .any(|v| v.voter_did == voter2 && v.vote_type == VoteType::Reject));
    assert!(proposal_votes_response
        .votes
        .iter()
        .any(|v| v.voter_did == voter3 && v.vote_type == VoteType::Abstain));

    let approve_count = proposal_votes_response.votes.iter().filter(|v| v.vote_type == VoteType::Approve).count();
    let reject_count = proposal_votes_response.votes.iter().filter(|v| v.vote_type == VoteType::Reject).count();
    let abstain_count = proposal_votes_response.votes.iter().filter(|v| v.vote_type == VoteType::Abstain).count();

    assert_eq!(approve_count, 1, "Approve votes should be 1 in full_lifecycle");
    assert_eq!(reject_count, 1, "Reject votes should be 1 in full_lifecycle");
    assert_eq!(abstain_count, 1, "Abstain votes should be 1 in full_lifecycle");

    println!("Proposal votes response: {:?}", proposal_votes_response);

    // Optional: Verify thread detail (if proposals are linked back to threads, which they are not in the current model)
    // let final_thread_detail = get_thread_detail(&client, &created_thread_summary.id).await;
    // Depending on whether ThreadDetail is updated to show linked proposals, add assertions here.
    // For now, we just check that the thread still exists.
    assert_eq!(thread_detail.summary.id, created_thread_summary.id);
}

#[tokio::test]
async fn test_get_threads_with_query_params() {
    let client = Client::new();

    // Create a couple of threads with different scopes
    let _ = create_thread(&client, "Thread A Scope X", "did:test:authorA", "scope.x").await;
    let _ = create_thread(&client, "Thread B Scope Y", "did:test:authorB", "scope.y").await;
    let _ = create_thread(&client, "Thread C Scope X", "did:test:authorC", "scope.x").await;

    // Test filtering by scope
    let threads_scope_x = client
        .get(format!("{}/threads?scope=scope.x", BASE_URL))
        .send()
        .await
        .expect("Failed request")
        .json::<Vec<ThreadSummary>>()
        .await
        .expect("Failed parse");

    assert!(
        threads_scope_x.len() >= 2,
        "Expected at least 2 threads with scope.x, found {}",
        threads_scope_x.len()
    );
    for thread in &threads_scope_x {
        assert_eq!(thread.scope, "scope.x");
    }
    println!("Threads with scope.x: {:?}", threads_scope_x.len());

    // Test limit
    let threads_limit_1 = client
        .get(format!("{}/threads?limit=1", BASE_URL))
        .send()
        .await
        .expect("Failed request")
        .json::<Vec<ThreadSummary>>()
        .await
        .expect("Failed parse");
    assert_eq!(threads_limit_1.len(), 1, "Expected 1 thread with limit=1");
    println!("Threads with limit 1: {:?}", threads_limit_1.len());
}

#[tokio::test]
async fn test_get_proposals_with_query_params() {
    let client = Client::new();
    let thread_summary = create_thread(
        &client,
        "Proposal Test Thread",
        "did:test:proposer",
        "proposal.test.scope",
    )
    .await;

    // Create some proposals
    let _p1 = create_proposal(
        &client,
        "Prop Alpha Open",
        "text",
        "gov.alpha",
        Some(thread_summary.id.clone()),
    )
    .await;
    let _p2 = create_proposal(
        &client,
        "Prop Beta Open",
        "text",
        "gov.beta",
        Some(thread_summary.id.clone()),
    )
    .await;

    // To test status, we'd need to be able to close proposals.
    // For now, we assume newly created ones are Open.

    // Test filtering by scope
    let proposals_gov_alpha = client
        .get(format!("{}/proposals?scope=gov.alpha", BASE_URL))
        .send()
        .await
        .expect("Failed request")
        .json::<Vec<icn_agoranet::models::ProposalSummary>>() // API returns ProposalSummary
        .await
        .expect("Failed parse");

    assert!(
        !proposals_gov_alpha.is_empty(),
        "Expected at least one proposal with scope gov.alpha"
    );
    for proposal in &proposals_gov_alpha {
        assert_eq!(proposal.scope, "gov.alpha");
    }
    println!(
        "Proposals with scope gov.alpha: {}",
        proposals_gov_alpha.len()
    );

    // Test filtering by status (all should be Open initially)
    let proposals_open = client
        .get(format!("{}/proposals?status=Open", BASE_URL))
        .send()
        .await
        .expect("Failed request")
        .json::<Vec<icn_agoranet::models::ProposalSummary>>()
        .await
        .expect("Failed parse");

    assert!(
        proposals_open.len() >= 2,
        "Expected at least 2 Open proposals, found {}",
        proposals_open.len()
    );
    for proposal in &proposals_open {
        assert_eq!(proposal.status, ProposalStatus::Open);
    }
    println!("Open proposals: {}", proposals_open.len());

    // Note: Filtering by `type` is not implemented in the handlers yet (it's a placeholder)
    // So we don't test it here.
}

#[tokio::test]
async fn test_get_proposal_votes_handler() {
    let (server_url, _handle, db) = spawn_app().await;
    let client = reqwest::Client::new();

    // 1. Create a proposal using the test helper
    let new_proposal_id = format!("proposal_{}", Uuid::new_v4());
    {
        let mut store = db.write().unwrap();
        store.add_proposal_for_test(ProposalDetail {
            summary: ProposalSummary {
                id: new_proposal_id.clone(),
                title: "Votes Test Proposal".to_string(),
                scope: "test.votes".to_string(),
                status: ProposalStatus::Open,
                vote_counts: VoteCounts { approve: 0, reject: 0, abstain: 0 }, // Initial counts
                voting_deadline: Utc::now() + Duration::days(1),
            },
            full_text: "Full text for votes test proposal".to_string(),
            linked_thread_id: None,
        });
    }

    // 2. Cast some votes via HTTP endpoint
    let voter1 = "did:example:voter1".to_string();
    let voter2 = "did:example:voter2".to_string();
    let voter3 = "did:example:voter3".to_string();

    for (voter_did, vote_type) in [
        (voter1.clone(), VoteType::Approve),
        (voter2.clone(), VoteType::Reject),
        (voter3.clone(), VoteType::Abstain),
    ] {
        let response = client
            .post(format!("{}/votes", server_url))
            .json(&NewVoteRequest {
                proposal_id: new_proposal_id.clone(),
                voter_did,
                vote_type,
                justification: Some("Test justification".to_string()),
            })
            .send()
            .await
            .expect("Failed to cast vote");
        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    }

    // 3. Get votes for the proposal
    let response = client
        .get(format!(
            "{}/proposals/{}/votes",
            server_url, new_proposal_id
        ))
        .send()
        .await
        .expect("Failed to get proposal votes");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let proposal_votes_response: ProposalVotesResponse = response
        .json()
        .await
        .expect("Failed to parse proposal votes response");

    assert_eq!(proposal_votes_response.proposal_id, new_proposal_id);
    assert_eq!(proposal_votes_response.votes.len(), 3);

    // Assert vote counts by iterating and filtering
    let approve_count = proposal_votes_response.votes.iter().filter(|v| v.vote_type == VoteType::Approve).count();
    let reject_count = proposal_votes_response.votes.iter().filter(|v| v.vote_type == VoteType::Reject).count();
    let abstain_count = proposal_votes_response.votes.iter().filter(|v| v.vote_type == VoteType::Abstain).count();

    assert_eq!(approve_count, 1, "Approve votes should be 1");
    assert_eq!(reject_count, 1, "Reject votes should be 1");
    assert_eq!(abstain_count, 1, "Abstain votes should be 1");
}

// TODO: Add more tests:
// - Error conditions (e.g., voting on a non-existent proposal, creating proposal for non-existent thread - though API might allow it)
// - Voting deadline enforcement (requires ability to manipulate time or wait, or set short deadlines)
// - Pagination if implemented for list endpoints beyond simple limit
