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

use icn_agoranet::models::{
    NewProposalRequest, NewThreadRequest, NewVoteRequest, ProposalDetail,
    ProposalStatus, ProposalVotesResponse, ThreadDetail, ThreadSummary, Vote, VoteType,
};
use reqwest::Client;
use serde_json::json; // For ad-hoc json creation in tests
use axum::{
    routing::{get, post},
    Router,
    Server,
};
use icn_agoranet::{
    handlers::{Db, InMemoryStore, create_proposal_handler, create_thread_handler, cast_vote_handler, get_proposal_detail_handler, get_threads_handler, health_check_handler, get_proposal_votes_handler},
    models::{NewProposalRequest, NewThreadRequest, NewVoteRequest, ProposalStatus, VoteType, Timestamp, VoteCounts, ThreadSummary, ProposalSummary, ProposalDetail},
};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;
use chrono::Utc;

const BASE_URL: &str = "http://127.0.0.1:8787";

// Helper to create a new thread
async fn create_thread(client: &Client, title: &str, author_did: &str, scope: &str) -> ThreadSummary {
    let req = NewThreadRequest {
        title: title.to_string(),
        author_did: author_did.to_string(),
        scope: scope.to_string(),
        metadata: Some(json!({"test_metadata": "some_value"})),
    };
    client
        .post(format!("{}/threads", BASE_URL))
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
    title: &str,
    full_text: &str,
    scope: &str,
    thread_id: Option<String>,
) -> ProposalDetail { // Assuming create returns ProposalDetail for easier access to ID
    let req = NewProposalRequest {
        title: title.to_string(),
        full_text: full_text.to_string(),
        scope: scope.to_string(),
        thread_id,
    };
    // The API actually returns ProposalSummary, but we fetch ProposalDetail immediately
    let summary = client
        .post(format!("{}/proposals", BASE_URL))
        .json(&req)
        .send()
        .await
        .expect("Failed to send create proposal request")
        .json::<icn_agoranet::models::ProposalSummary>()
        .await
        .expect("Failed to parse create proposal response");

    // Fetch the ProposalDetail to get all fields, including the ID.
    get_proposal_detail(client, &summary.id).await
}

// Helper to get proposal detail
async fn get_proposal_detail(client: &Client, proposal_id: &str) -> ProposalDetail {
    client
        .get(format!("{}/proposals/{}", BASE_URL, proposal_id))
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
        .post(format!("{}/votes", BASE_URL))
        .json(&req)
        .send()
        .await
        .expect("Failed to send cast vote request")
        .json::<Vote>()
        .await
        .expect("Failed to parse cast vote response")
}

// Helper to get proposal votes
async fn get_proposal_votes(client: &Client, proposal_id: &str) -> ProposalVotesResponse {
    client
        .get(format!("{}/votes/{}", BASE_URL, proposal_id))
        .send()
        .await
        .expect("Failed to send get proposal votes request")
        .json::<ProposalVotesResponse>()
        .await
        .expect("Failed to parse get proposal votes response")
}

// Helper to get thread detail
async fn get_thread_detail(client: &Client, thread_id: &str) -> ThreadDetail {
    client
        .get(format!("{}/threads/{}", BASE_URL, thread_id))
        .send()
        .await
        .expect("Failed to send get thread detail request")
        .json::<ThreadDetail>()
        .await
        .expect("Failed to parse get thread detail response")
}

// Helper to spawn a test server
async fn spawn_test_server() -> (JoinHandle<()>, String) {
    let store = Arc::new(RwLock::new(InMemoryStore::new()));
    let app = Router::new()
        .route("/health", get(health_check_handler))
        .route("/threads", get(get_threads_handler).post(create_thread_handler))
        // .route("/threads/:id", get(get_thread_detail_handler)) // Assuming get_thread_detail_handler exists
        .route("/proposals", get(icn_agoranet::handlers::get_proposals_handler).post(create_proposal_handler))
        .route("/proposals/:id", get(get_proposal_detail_handler))
        .route("/proposals/:id/votes", get(get_proposal_votes_handler))
        .route("/votes", post(cast_vote_handler))
        .with_state(store.clone() as Db);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service()).await.unwrap();
    });

    (handle, base_url)
}

#[tokio::test]
async fn test_full_lifecycle() {
    let client = Client::new();

    // 1. Create a new thread
    let thread_title = "Integration Test Thread";
    let thread_author = "did:test:thread_author";
    let thread_scope = "test.scope.thread";
    let created_thread_summary = create_thread(&client, thread_title, thread_author, thread_scope).await;

    assert_eq!(created_thread_summary.title, thread_title);
    assert_eq!(created_thread_summary.author_did, thread_author);
    assert_eq!(created_thread_summary.scope, thread_scope);
    println!("Created thread: {}", created_thread_summary.id);

    // Fetch thread detail to verify
    let thread_detail = get_thread_detail(&client, &created_thread_summary.id).await;
    assert_eq!(thread_detail.summary.id, created_thread_summary.id);
    assert_eq!(thread_detail.messages.len(), 0); // New threads have no messages initially (as per current model)

    // 2. Create a new proposal linked to that thread
    let proposal_title = "Integration Test Proposal";
    let proposal_text = "This is a detailed proposal for integration testing.";
    let proposal_scope = "test.scope.proposal";
    let created_proposal_detail = create_proposal(
        &client,
        proposal_title,
        proposal_text,
        proposal_scope,
        Some(created_thread_summary.id.clone()),
    )
    .await;

    assert_eq!(created_proposal_detail.summary.title, proposal_title);
    assert_eq!(created_proposal_detail.full_text, proposal_text);
    assert_eq!(created_proposal_detail.summary.scope, proposal_scope);
    assert_eq!(created_proposal_detail.linked_thread_id, Some(created_thread_summary.id.clone()));
    assert_eq!(created_proposal_detail.summary.status, ProposalStatus::Open);
    assert_eq!(created_proposal_detail.summary.vote_counts.approve, 0);
    println!("Created proposal: {}", created_proposal_detail.summary.id);


    // 3. Cast a few votes on the proposal
    let voter1 = "did:test:voter1";
    let voter2 = "did:test:voter2";
    let voter3 = "did:test:voter3";

    let vote1 = cast_vote(
        &client,
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
    let updated_proposal_detail = get_proposal_detail(&client, &created_proposal_detail.summary.id).await;
    assert_eq!(updated_proposal_detail.summary.vote_counts.approve, 1);
    assert_eq!(updated_proposal_detail.summary.vote_counts.reject, 1);
    assert_eq!(updated_proposal_detail.summary.vote_counts.abstain, 1);
    println!("Updated proposal detail vote counts: {:?}", updated_proposal_detail.summary.vote_counts);


    // 5. Get the proposal votes to see individual votes and summary
    let proposal_votes_response = get_proposal_votes(&client, &created_proposal_detail.summary.id).await;
    assert_eq!(proposal_votes_response.votes.len(), 3);
    assert!(proposal_votes_response.votes.iter().any(|v| v.voter_did == voter1 && v.vote_type == VoteType::Approve));
    assert!(proposal_votes_response.votes.iter().any(|v| v.voter_did == voter2 && v.vote_type == VoteType::Reject));
    assert!(proposal_votes_response.votes.iter().any(|v| v.voter_did == voter3 && v.vote_type == VoteType::Abstain));
    
    assert_eq!(proposal_votes_response.summary.approve, 1);
    assert_eq!(proposal_votes_response.summary.reject, 1);
    assert_eq!(proposal_votes_response.summary.abstain, 1);
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
    
    assert!(threads_scope_x.len() >= 2, "Expected at least 2 threads with scope.x, found {}", threads_scope_x.len());
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
    let thread_summary = create_thread(&client, "Proposal Test Thread", "did:test:proposer", "proposal.test.scope").await;


    // Create some proposals
    let _p1 = create_proposal(&client, "Prop Alpha Open", "text", "gov.alpha", Some(thread_summary.id.clone())).await;
    let _p2 = create_proposal(&client, "Prop Beta Open", "text", "gov.beta", Some(thread_summary.id.clone())).await;
    
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

    assert!(proposals_gov_alpha.len() >= 1, "Expected at least 1 proposal with scope gov.alpha, found {}", proposals_gov_alpha.len());
    for proposal in &proposals_gov_alpha {
        assert_eq!(proposal.scope, "gov.alpha");
    }
    println!("Proposals with scope gov.alpha: {}", proposals_gov_alpha.len());


    // Test filtering by status (all should be Open initially)
    let proposals_open = client
        .get(format!("{}/proposals?status=Open", BASE_URL))
        .send()
        .await
        .expect("Failed request")
        .json::<Vec<icn_agoranet::models::ProposalSummary>>()
        .await
        .expect("Failed parse");
    
    assert!(proposals_open.len() >= 2, "Expected at least 2 Open proposals, found {}", proposals_open.len());
    for proposal in &proposals_open {
        assert_eq!(proposal.status, ProposalStatus::Open);
    }
     println!("Open proposals: {}", proposals_open.len());

    // Note: Filtering by `type` is not implemented in the handlers yet (it's a placeholder)
    // So we don't test it here.
}

#[tokio::test]
async fn thread_proposal_vote_flow() {
    let (_server_handle, base_url) = spawn_test_server().await;
    let client = Client::new();

    let scope = "test.scope".to_string();
    let author_did = "did:test:author".to_string();

    // 1. POST /threads -> assert 201
    let new_thread_req = NewThreadRequest {
        title: "Test Thread for Integration Flow".to_string(),
        author_did: author_did.clone(),
        scope: scope.clone(),
        metadata: None,
    };
    let res = client.post(format!("{}/threads", base_url))
        .json(&new_thread_req)
        .send()
        .await
        .expect("Failed to create thread");
    assert_eq!(res.status(), reqwest::StatusCode::CREATED);
    let thread: ThreadSummary = res.json().await.expect("Failed to parse thread summary");
    assert_eq!(thread.title, new_thread_req.title);
    let thread_id = thread.id.clone();

    // 2. POST /proposals -> assert 201
    let new_proposal_req = NewProposalRequest {
        title: "Test Proposal for Integration Flow".to_string(),
        full_text: "This is a detailed description of the test proposal.".to_string(),
        scope: scope.clone(),
        linked_thread_id: Some(thread_id.clone()),
        voting_deadline: Some(Utc::now() + chrono::Duration::days(1)),
    };
    let res = client.post(format!("{}/proposals", base_url))
        .json(&new_proposal_req)
        .send()
        .await
        .expect("Failed to create proposal");
    assert_eq!(res.status(), reqwest::StatusCode::CREATED);
    let proposal: ProposalSummary = res.json().await.expect("Failed to parse proposal summary");
    assert_eq!(proposal.title, new_proposal_req.title);
    let proposal_id = proposal.id.clone();

    // 3. POST /votes -> cast Approve & Reject
    let voter1_did = "did:test:voter1".to_string();
    let approve_vote_req = NewVoteRequest {
        proposal_id: proposal_id.clone(),
        voter_did: voter1_did.clone(),
        vote_type: VoteType::Approve,
        justification: Some("I approve this test proposal".to_string()),
    };
    let res = client.post(format!("{}/votes", base_url))
        .json(&approve_vote_req)
        .send()
        .await
        .expect("Failed to cast approve vote");
    assert_eq!(res.status(), reqwest::StatusCode::CREATED);

    let voter2_did = "did:test:voter2".to_string();
    let reject_vote_req = NewVoteRequest {
        proposal_id: proposal_id.clone(),
        voter_did: voter2_did.clone(),
        vote_type: VoteType::Reject,
        justification: Some("I reject this test proposal".to_string()),
    };
    let res = client.post(format!("{}/votes", base_url))
        .json(&reject_vote_req)
        .send()
        .await
        .expect("Failed to cast reject vote");
    assert_eq!(res.status(), reqwest::StatusCode::CREATED);

    // 4. GET /proposals/:id -> assert vote_counts updated
    let res = client.get(format!("{}/proposals/{}", base_url, proposal_id))
        .send()
        .await
        .expect("Failed to get proposal detail");
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let proposal_detail: ProposalDetail = res.json().await.expect("Failed to parse proposal detail");
    assert_eq!(proposal_detail.summary.vote_counts.approve, 1);
    assert_eq!(proposal_detail.summary.vote_counts.reject, 1);
    assert_eq!(proposal_detail.summary.vote_counts.abstain, 0);

    // 5. GET /threads -> assert thread list length >= 1 (can be more if InMemoryStore is not reset)
    let res = client.get(format!("{}/threads?scope={}", base_url, scope))
        .send()
        .await
        .expect("Failed to get threads");
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let threads: Vec<ThreadSummary> = res.json().await.expect("Failed to parse thread list");
    assert!(!threads.is_empty()); 
    // More specific check if we know the exact number or can filter by specific ID
    assert!(threads.iter().any(|t| t.id == thread_id));

    // TODO: Add assertions for GET /proposals/:id/votes to check the actual votes if needed
}

// TODO: Add more tests:
// - Error conditions (e.g., voting on a non-existent proposal, creating proposal for non-existent thread - though API might allow it)
// - Voting deadline enforcement (requires ability to manipulate time or wait, or set short deadlines)
// - Pagination if implemented for list endpoints beyond simple limit 