use icn_identity::Did;
use icn_runtime::context::{RuntimeContext, RuntimeContextBuilder};
use icn_runtime::host_environment::ConcreteHostEnvironment;
use icn_runtime::job_execution_context::JobExecutionContext;
use icn_economics::{
    ManaRepositoryAdapter, ResourcePolicyEnforcer, ResourceAuthorization
};
use icn_economics::mana::{ManaLedger, ManaState, InMemoryManaLedger};
use icn_types::mesh::MeshJobParams;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use host_abi::{HostAbiError, MeshHostAbi};

#[tokio::test]
async fn test_host_account_spend_mana_allows_when_quota_and_balance_sufficient() {
    let ledger_arc = Arc::new(InMemoryManaLedger::new());

    let did = Did::from_str("did:coop:example:alice").unwrap();
    ledger_arc.update_mana_state(&did, ManaState {
        current_mana: 100,
        max_mana: 100,
        regen_rate_per_epoch: 0.0,
        last_updated_epoch: 0,
    }).await.unwrap();

    let mana_repo_adapter_arc = Arc::new(ManaRepositoryAdapter::new(ledger_arc.clone()));
    
    let mut enforcer_instance = ResourcePolicyEnforcer::new(Box::new(mana_repo_adapter_arc.clone() as Box<dyn icn_economics::policy::ResourceRepositoryReader + Send + Sync>));
    enforcer_instance.set_policy("mana", &format!("did:{}", did), ResourceAuthorization::Quota(100));
    let policy_enforcer_arc = Arc::new(enforcer_instance);

    let rt = Arc::new(RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_executor_id(did.to_string())
        .with_policy_enforcer(policy_enforcer_arc.clone())
        .with_mana_repository(mana_repo_adapter_arc.clone())
        .build());

    let job_exec_params = MeshJobParams { ..Default::default() };
    let job_ctx = JobExecutionContext::new(
        "job:test:alice".to_string(),
        did.clone(),
        job_exec_params,
        did.clone(),
        0,
    );
    let job_ctx_arc = Arc::new(Mutex::new(job_ctx));

    let env = ConcreteHostEnvironment::new(job_ctx_arc, did.clone(), rt);

    let result = MeshHostAbi::host_account_spend_mana(&env, MockCaller, 0, 0, 30).await.unwrap();
    assert_eq!(result, 0);

    let new_balance = MeshHostAbi::host_account_get_mana(&env, MockCaller, 0, 0).await.unwrap();
    assert_eq!(new_balance, 70);
}

#[tokio::test]
async fn test_host_account_spend_mana_denied_when_exceeds_quota() {
    let ledger_arc = Arc::new(InMemoryManaLedger::new());

    let did = Did::from_str("did:coop:example:bob").unwrap();
    ledger_arc.update_mana_state(&did, ManaState {
        current_mana: 100,
        max_mana: 100,
        regen_rate_per_epoch: 0.0,
        last_updated_epoch: 0,
    }).await.unwrap();

    let mana_repo_adapter_arc = Arc::new(ManaRepositoryAdapter::new(ledger_arc.clone()));
    
    let mut enforcer_instance = ResourcePolicyEnforcer::new(Box::new(mana_repo_adapter_arc.clone() as Box<dyn icn_economics::policy::ResourceRepositoryReader + Send + Sync>));
    enforcer_instance.set_policy("mana", &format!("did:{}", did), ResourceAuthorization::Quota(40));
    let policy_enforcer_arc = Arc::new(enforcer_instance);
    
    let rt = Arc::new(RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_executor_id(did.to_string())
        .with_policy_enforcer(policy_enforcer_arc.clone())
        .with_mana_repository(mana_repo_adapter_arc.clone())
        .build());

    let job_exec_params = MeshJobParams { ..Default::default() };
    let job_ctx = JobExecutionContext::new(
        "job:test:bob".to_string(),
        did.clone(),
        job_exec_params,
        did.clone(),
        0,
    );
    let job_ctx_arc = Arc::new(Mutex::new(job_ctx));

    let env = ConcreteHostEnvironment::new(job_ctx_arc, did.clone(), rt);

    let result = MeshHostAbi::host_account_spend_mana(&env, MockCaller, 0, 0, 50).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), HostAbiError::ResourceLimitExceeded as i32);
}

#[tokio::test]
async fn test_host_account_spend_mana_denied_when_insufficient_balance() {
    let ledger_arc = Arc::new(InMemoryManaLedger::new());

    let did_str = "did:coop:example:charlie";
    let did = Did::from_str(did_str).unwrap();
    ledger_arc.update_mana_state(&did, ManaState {
        current_mana: 20,
        max_mana: 100,
        regen_rate_per_epoch: 0.0,
        last_updated_epoch: 0,
    }).await.unwrap();

    let mana_repo_adapter_arc = Arc::new(ManaRepositoryAdapter::new(ledger_arc.clone()));
    
    let mut enforcer_instance = ResourcePolicyEnforcer::new(Box::new(mana_repo_adapter_arc.clone() as Box<dyn icn_economics::policy::ResourceRepositoryReader + Send + Sync>));
    enforcer_instance.set_policy("mana", &format!("did:{}", did), ResourceAuthorization::Quota(100));
    let policy_enforcer_arc = Arc::new(enforcer_instance);

    let rt = Arc::new(RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_executor_id(did.to_string())
        .with_policy_enforcer(policy_enforcer_arc.clone())
        .with_mana_repository(mana_repo_adapter_arc.clone())
        .build());
    
    let job_exec_params = MeshJobParams { ..Default::default() };
    let job_ctx = JobExecutionContext::new(
        "job:test:charlie".to_string(),
        did.clone(),
        job_exec_params,
        did.clone(),
        0,
    );
    let job_ctx_arc = Arc::new(Mutex::new(job_ctx));

    let env = ConcreteHostEnvironment::new(job_ctx_arc, did.clone(), rt);

    let result = MeshHostAbi::host_account_spend_mana(&env, MockCaller, 0, 0, 30).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), HostAbiError::ResourceLimitExceeded as i32);

    let current_balance = MeshHostAbi::host_account_get_mana(&env, MockCaller, 0, 0).await.unwrap();
    assert_eq!(current_balance, 20);
}

struct MockCaller;

#[tokio::test]
async fn test_host_account_get_mana_uses_caller_did() {
    let ledger_arc = Arc::new(InMemoryManaLedger::new());

    let caller_did = Did::from_str("did:coop:example:alice").unwrap();
    ledger_arc.update_mana_state(&caller_did, ManaState {
        current_mana: 77,
        max_mana: 100,
        regen_rate_per_epoch: 0.0,
        last_updated_epoch: 0,
    }).await.unwrap();

    let mana_repo_adapter_arc = Arc::new(ManaRepositoryAdapter::new(ledger_arc.clone()));
    let mut enforcer_instance = ResourcePolicyEnforcer::new(Box::new(mana_repo_adapter_arc.clone() as Box<dyn icn_economics::policy::ResourceRepositoryReader + Send + Sync>));
    let policy_enforcer_arc = Arc::new(enforcer_instance);

    let rt = Arc::new(RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_executor_id(caller_did.to_string())
        .with_policy_enforcer(policy_enforcer_arc.clone())
        .with_mana_repository(mana_repo_adapter_arc.clone())
        .build());

    let job_params = MeshJobParams { ..Default::default() };
    let job_ctx = JobExecutionContext::new(
        "job:test:getmanacaller".to_string(),
        caller_did.clone(),
        job_params,
        caller_did.clone(),
        0,
    );
    let job_ctx_arc = Arc::new(Mutex::new(job_ctx));

    let env = ConcreteHostEnvironment::new(job_ctx_arc, caller_did.clone(), rt);

    let balance = MeshHostAbi::host_account_get_mana(&env, MockCaller, 0, 0).await.unwrap();
    assert_eq!(balance, 77);
}

// TODO: Implement test_host_account_spend_mana_with_scope_variation
// This will also require solving the ConcreteHostEnvironment instantiation and ABI call mechanism,
// especially if did_ptr/did_len are non-zero, which would require actual memory interaction
// and a more functional MockCaller or wasmtime::Caller setup. 