## 7.2 Role Definitions and Permission Gates: "Treasury Operations"

This example demonstrates how CCL can be used to define distinct roles within a cooperative and enforce permission checks based on the calling agent's assigned role before executing sensitive actions. It also shows how to handle unauthorized attempts gracefully.

### 7.2.1 CCL Source: `treasury_operations.ccl`

```ccl
// treasury_operations.ccl

// --- Role Definitions ---
// Establishes the available roles and their associated permissions.
// This definition would typically be part of a broader governance contract.
roles_def "CooperativeRoles" {
  role "treasurer" {
    description "Manages cooperative funds and financial records."
    permissions [
      "treasury.allocate_budget", 
      "treasury.view_financials",
      "treasury.disburse_funds" 
    ]
  }
  role "member" {
    description "Standard member of the cooperative."
    permissions [
      "proposal.submit", 
      "proposal.vote",
      "general.view_public_records"
    ]
  }
  role "auditor" {
    description "External or internal auditor with read-only access to financial records."
    permissions [
      "treasury.view_financials",
      "governance.view_vote_tallies"
    ]
  }
}

// --- Action Handler for Budget Allocation ---
// This process defines how a specific action, "request_budget_allocation", is handled.
// It uses role-based permission checks.
action_handler "HandleBudgetAllocationRequest" {
  description "Processes requests for allocating funds from a specific budget category."
  
  // Assumed context variables available at runtime for this action:
  // - caller.id: string (DID of the agent invoking the action)
  // - caller.role: string (Role of the agent, resolved by the host based on caller.id and active roles_def)
  // - request.amount: number (Amount requested for allocation)
  // - request.category: string (Budget category, e.g., "operations", "development")
  // - request.justification_cid: string (CID of a document justifying the request)

  triggered_by "budget.allocation.requested" // Event name that triggers this handler

  steps {
    // Step 1: Permission Check
    check_permission {
      required_permission "treasury.allocate_budget" 
      // Implicitly uses caller.role and the active roles_def
      // If fails, the host might halt or the CCL can define fallback behavior.
      // For this example, we assume the host halts or a specific error is raised if check_permission fails.
    }

    // Step 2: Validate Request (simplified)
    // A real scenario would involve more complex validation.
    if request.amount <= 0 {
      log_event(
        name: "budget_allocation.failed",
        level: "error",
        detail: "Requested amount must be positive.",
        context: { request_id: request.id, reason: "InvalidAmount" }
      );
      fail_action("Invalid request amount."); // Explicitly fail the action
    }

    // Step 3: Perform Metered Action (if permission granted and validation passed)
    perform_metered_action("ProcessBudgetAllocation", ResourceType.Transaction, 50);

    // Step 4: Anchor a record of the allocation
    anchor_data {
      path concat("treasury/allocations/", request.category, "/", request.id),
      data {
        allocated_to: caller.id, // Could be a different target if specified
        amount: request.amount,
        category: request.category,
        justification_cid: request.justification_cid,
        timestamp: timestamp()
      },
      metadata { contentType: "application/vnd.icn.treasury.allocation.v1+json" }
    }

    // Step 5: Issue a receipt token
    mint_token {
      token_type "BudgetAllocationReceipt"
      recipient caller.id // Or a designated recipient for the allocation
      amount    1
      data {
        request_id: request.id,
        amount: request.amount,
        category: request.category,
        allocated_at: timestamp()
      }
    }

    // Step 6: Log success
    log_event(
      name: "budget_allocation.succeeded",
      detail: "Budget allocation processed successfully.",
      context: { request_id: request.id, amount: request.amount, category: request.category }
    );
  }

  // --- Fallback for Unauthorized Access ---
  // This block could be invoked by the runtime if the initial `check_permission` fails,
  // OR if the `if caller.has_permission(...)` (alternative style) evaluates to false.
  // The exact mechanism depends on the `check_permission` primitive's behavior.
  // For this example, we'll use an alternative explicit check for illustration if `check_permission` isn't a halting primitive.

  // Alternative/Additional Handling (if check_permission isn't halting or for more nuanced logic):
  /* 
  // This illustrates an alternative way if `check_permission` is not halting.
  // The `steps` block would then be wrapped in an `if caller.has_permission(...)`.
  
  on_fail (type: "PermissionDenied") { // Conceptual: triggered if check_permission fails and sets a specific error type
     log_event(
        name: "budget_allocation.unauthorized",
        level: "warn",
        detail: concat("Caller ", caller.id, " lacks 'treasury.allocate_budget' permission."),
        context: { request_id: request.id, attempted_action: "treasury.allocate_budget" }
     );
     // No further action taken, request is effectively denied.
  }
  */
}
```

### 7.2.2 Purpose & Overview

This CCL contract demonstrates two main concepts:
1.  **Role Definition (`roles_def`)**: How to declare different roles within a system (e.g., "treasurer," "member," "auditor") and associate a list of specific named permissions with each role.
2.  **Permission Gating (`check_permission` or `caller.has_permission`)**: How an `action_handler` can guard critical operations by verifying that the invoking agent (`caller`) possesses the necessary permission as defined in the active `roles_def`. It also shows logging for unauthorized attempts.

The scenario involves a "HandleBudgetAllocationRequest" action that should only be executable by a "treasurer."

### 7.2.3 Key Constructs & Semantics

*   **`roles_def "<Name>" { ... }`**:
    *   Defines a set of roles.
    *   Each `role "<RoleName>" { ... }` block specifies:
        *   `description`: Human-readable description of the role.
        *   `permissions [...]`: An array of strings, where each string is a named permission (e.g., "treasury.allocate_budget").
*   **`action_handler "<Name>" { ... }`**:
    *   Defines a sequence of operations to be performed when a specific event (`triggered_by`) occurs.
    *   `triggered_by`: Specifies the event that invokes this handler.
    *   `steps { ... }`: Contains the primary logic of the action.
*   **`check_permission { required_permission "<PermissionName>" }`**:
    *   This is a dedicated CCL primitive (or a host-provided function that acts like one).
    *   It implicitly checks if the `caller.role` (provided by the host context) has the `required_permission` according to the currently active `roles_def`.
    *   If the permission is not granted, this step would typically cause the action to halt or raise a specific error that could be caught by an `on_fail` block (as conceptualized). The exact behavior (halting vs. error raising) is a design choice for the CCL runtime environment.
*   **`if <condition>` with `caller.has_permission("<PermissionName>")` (Alternative Style)**:
    *   While the example primarily uses `check_permission`, an alternative or complementary approach is a boolean function like `caller.has_permission("permission.name")` that can be used in standard `if` conditions. This allows for more complex logic (e.g., granting access if a user has *any one of* a list of permissions).
*   **Context Variables**:
    *   `caller.id`: The DID of the agent invoking the action.
    *   `caller.role`: The role of the agent, resolved by the host against an active `roles_def`. This is crucial for permission checks.
    *   `request.*`: Data associated with the triggering event/request.
*   **`fail_action("<Reason>")`**: An explicit CCL primitive to stop the current action handler's execution and mark it as failed, potentially with a reason.
*   **`log_event(...)`, `perform_metered_action(...)`, `anchor_data {...}`, `mint_token {...}`**: Standard actions as seen in previous examples.
*   **`on_fail (type: "PermissionDenied") { ... }` (Conceptual)**:
    *   This illustrates a potential CCL construct for declarative error handling or specific failure cases. If `check_permission` (or another step) fails in a way that sets a "PermissionDenied" error type, this block would be executed. This is more advanced and depends on the CCL error handling design.

### 7.2.4 Conceptual DSL AST Mapping (`icn-ccl-dsl`) (Excerpt)

*   **`roles_def "CooperativeRoles" { ... }` block:**
    ```rust
    DslModule::RolesDef {
        name: "CooperativeRoles".to_string(),
        roles: vec![
            RoleDef {
                name: "treasurer".to_string(),
                description: Some("Manages cooperative funds...".to_string()),
                permissions: vec![
                    "treasury.allocate_budget".to_string(), 
                    "treasury.view_financials".to_string(),
                    "treasury.disburse_funds".to_string(),
                ],
            },
            RoleDef {
                name: "member".to_string(),
                description: Some("Standard member of the cooperative.".to_string()),
                permissions: vec![
                    "proposal.submit".to_string(), 
                    "proposal.vote".to_string(),
                    "general.view_public_records".to_string(),
                ],
            },
            // ... other roles ...
        ],
    }
    ```

*   **`action_handler "HandleBudgetAllocationRequest" { ... }` block:**
    This would map to a `DslModule::ActionHandler` or similar structure:
    ```rust
    DslModule::ActionHandler {
        name: "HandleBudgetAllocationRequest".to_string(),
        description: Some("Processes requests for allocating funds...".to_string()),
        triggered_by: "budget.allocation.requested".to_string(),
        steps: vec![
            // Step 1: check_permission
            ActionStep::CheckPermission { // Or a generic FunctionCall
                required_permission: "treasury.allocate_budget".to_string(),
            },
            // Step 2: if request.amount <= 0 { ... fail_action ... }
            ActionStep::If(IfExpr {
                condition_raw: "request.amount <= 0".to_string(), // Or parsed condition
                then_rules: vec![ // Representing the then block
                    RuleValue::Map(vec![ // log_event
                        Rule { key: "log_event".to_string(), value: RuleValue::Map(...) },
                    ]),
                    RuleValue::Map(vec![ // fail_action
                        Rule { key: "fail_action".to_string(), value: RuleValue::String("Invalid request amount.".to_string()) },
                    ]),
                ],
                else_rules: None,
            }),
            // Step 3: perform_metered_action
            ActionStep::PerformMeteredAction {
                action_name: "ProcessBudgetAllocation".to_string(),
                resource_type: "Transaction".to_string(), // Assuming ResourceType enum
                amount: 50,
            },
            // Step 4: anchor_data
            ActionStep::Anchor { path_expr: Some(...), data_ref: None, content_ref: Some(RuleValue::Map(...)) , metadata: Some(...) },
            // Step 5: mint_token
            ActionStep::MintToken { token_type: "BudgetAllocationReceipt".to_string(), recipient_ref: Some("caller.id".to_string()), amount_val: Some(1), data_expr: Some(RuleValue::Map(...)) },
            // Step 6: log_event (success)
            ActionStep::Log(LogParams { name: "budget_allocation.succeeded".to_string(), ... }), // Assuming specific Log ActionStep
        ],
        // Conceptual on_fail block
        // on_fail: Some(vec![ OnFailBlock { error_type: "PermissionDenied", steps: vec![ ...log_event... ] } ]),
    }
    ```

### 7.2.5 Conceptual Opcode Sequence (`icn-ccl-wasm-codegen`)

A simplified conceptual sequence for the `action_handler`:

```rust
// Program.ops (for HandleBudgetAllocationRequest):
[
    // Opcode::SetActionHandlerContext { handler_name: "HandleBudgetAllocationRequest", trigger: "budget.allocation.requested" },

    // Step 1: Permission Check
    Opcode::CheckPermission { permission_name_ptr: ..., permission_name_len: ... }, // "treasury.allocate_budget"
    // This opcode would cause the host to check against caller.role and roles_def.
    // If it fails, the host might trap, or set a flag that subsequent opcodes check.
    // For a non-halting version, it might return a boolean to a register.

    // Step 2: Validate Request
    Opcode::IfConditionExpr { condition_expr_string: "request.amount <= 0" }, // Host evaluates this with context
        Opcode::CallHostFunction { function_name: "log_event", args_json: "{...}" },
        Opcode::FailAction { reason_ptr: ..., reason_len: ... }, // "Invalid request amount." - Halts this action
    Opcode::EndIf, // No Else branch for this specific If

    // Step 3: Perform Metered Action
    Opcode::UseResource { resource_type: "Transaction", amount: 50 }, // "ProcessBudgetAllocation" is descriptive

    // Step 4: Anchor Data
    Opcode::AnchorData { 
        path_expr: "concat("treasury/allocations/", request.category, "/", request.id)",
        data_json_expr: "{"allocated_to": caller.id, ... , "timestamp": timestamp()}", // Host resolves expr
        metadata_json: "{"contentType":"application/vnd.icn.treasury.allocation.v1+json"}"
    },

    // Step 5: Mint Token
    Opcode::MintToken {
        token_type: "BudgetAllocationReceipt",
        recipient_ref: "caller.id",
        amount: 1,
        data_json_expr: "{"request_id": request.id, ... , "allocated_at": timestamp()}"
    },

    // Step 6: Log Success
    Opcode::CallHostFunction { function_name: "log_event", args_json: "{"name":"budget_allocation.succeeded", ...}" },

    // Conceptual: Opcodes for on_fail (if not handled by host trap on CheckPermission failure)
    // Opcode::IfErrorCondition { error_type: "PermissionDenied" }, // Checks a flag set by a failing CheckPermission
    //     Opcode::CallHostFunction { function_name: "log_event", args_json: "{"name":"budget_allocation.unauthorized", ...}" },
    // Opcode::EndIfError,
]
```
The `roles_def` itself might be compiled into a data structure that the host loads or that `CheckPermission` opcodes refer to by name ("CooperativeRoles").

### 7.2.6 Runtime (Host-ABI) Calls Invoked

| CCL Action / Construct             | Conceptual Host ABI Interaction(s)                                                                                                                               | Key Parameters Passed from WASM (Illustrative)                                                                                                                                                                 |
| :--------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `roles_def` (loading/activation)   | `host_load_roles_definition(roles_def_json_ptr: u32, roles_def_json_len: u32)` (Called once to establish active roles)                                        | Pointer to serialized JSON or CBOR of the `roles_def` structure.                                                                                                                                             |
| `check_permission { ... }`         | `host_check_permission(caller_did_ptr: u32, ..., permission_name_ptr: u32, ...) -> i32` (0=granted, error code if not)                                          | Pointers to caller's DID (host resolves role) and the required permission string. Host uses active `roles_def`.                                                                                             |
| `caller.has_permission(...)` (alt) | `host_evaluate_condition(condition_str_ptr: u32, ...)` where condition includes `has_permission(...)` which the host parses.                                    | Pointer to condition string.                                                                                                                                                                                 |
| `fail_action("<Reason>")`          | `host_signal_action_failure(reason_ptr: u32, reason_len: u32)`                                                                                                    | Pointer to UTF-8 reason string. Causes the host to mark the current action execution as failed.                                                                                                            |
| `log_event(...)`                   | `host_log_message(...)`                                                                                                                                          | As in previous example.                                                                                                                                                                                      |
| `perform_metered_action(...)`      | `host_check_resource_authorization(...)`, `host_record_resource_usage(...)`                                                                                      | As in previous example. `ResourceType.Transaction` would map to a u32.                                                                                                                                       |
| `anchor_data {...}`                | `host_anchor_to_dag(...)`                                                                                                                                        | As in previous example. Host resolves context vars like `request.id`, `caller.id`, `timestamp()`.                                                                                                              |
| `mint_token {...}`                 | `host_mint_token(...)`                                                                                                                                           | As in previous example. Host resolves `caller.id`, `timestamp()`.                                                                                                                                              |
| Accessing context (e.g. `caller.id`, `request.amount`) | `host_get_context_value(...)`                                                                                                                | As in previous example.                                                                                                                                                                                      |

### 7.2.7 Notes on Role Resolution and Context

*   **Role Context**: A crucial aspect is how `caller.role` is determined by the host. When an agent identified by `caller.id` (a DID) invokes an action, the `icn-runtime` would need to:
    1.  Identify the relevant `roles_def` contract applicable to the current execution scope (e.g., the cooperative or organization instance).
    2.  Look up the `caller.id` within that cooperative's membership records, which should specify the agent's role(s).
    3.  Provide this role string (e.g., "treasurer") as part of the execution context to the WASM module.
*   **Multiple Roles**: If an agent can have multiple roles, the `check_permission` logic (either in the host or via multiple `Opcode::CheckPermissionForRole` calls) would need to iterate through the agent's assigned roles to see if any of them grant the required permission. CCL syntax might need to evolve to handle "any of" or "all of" multiple permissions if this becomes complex.
*   **Dynamic Roles**: The `roles_def` itself could be a CCL contract that is updatable via a governance process, allowing permissions and roles to evolve over time. The `host_load_roles_definition` would then point to the currently active version.

### 7.2.8 Error Handling for Permission Denied

*   If `check_permission` is a halting operation (i.e., it traps WASM execution on failure), the `icn-runtime` would catch this trap and could log the "Permission Denied" event.
*   If `check_permission` returns a status code, the generated WASM would include opcodes to check this status. If access is denied, it could then execute specific CCL logic (like the conceptual `on_fail` block or an `if/else` checking the status) to log the event or take other non-halting actions. The current example uses `check_permission` as potentially halting, with `fail_action` for explicit business logic failures. 