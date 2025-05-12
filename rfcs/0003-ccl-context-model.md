# RFC-0003: CCL Execution Context Model

**Status:** Proposed
**Author(s):** ICN AI Assistant, ICN Development Team
**Date:** (Current Date)

## 1. Introduction

This section defines the model for the execution context available to Cooperative Contract Language (CCL) contracts when they are run by the ICN runtime. The context provides CCL instances with necessary information about their environment, the trigger for their execution, and relevant system state. A clear and consistent context model is crucial for writing predictable, robust, and portable CCL contracts.

## 2. Context Access in CCL

CCL contracts access contextual information using a dot-notation syntax, referencing namespaces and their properties. For example, the DID of the entity invoking the contract might be accessed as `caller.id`, and the current system timestamp as `system.timestamp_unix`.

All context keys and property names **MUST** be valid CCL identifiers (alphanumeric with underscores, no dots or control characters). Dot notation represents navigation between objects and their properties, not literal key names containing dots.

The CCL runtime is responsible for populating these namespaces and making them accessible to the executing WASM module. The underlying mechanism might involve a combination of pre-initialized memory segments (for relatively static context) and host ABI calls (for dynamic or sensitive information), but this complexity is abstracted away from the CCL author by the context access syntax.

## 3. Core Context Namespaces

The following core namespaces are proposed. For each property, its Name, Data Type, Presence (Guaranteed or Optional), and Description are provided. (Canonical types like `DID`, `TimestampValue`, etc., are assumed to be defined in a separate `ICN Canonical Types RFC`).

### 3.1. `system` Namespace

*   **Purpose:** Provides information about the current ICN runtime environment and blockchain state.
*   **Properties:**
    *   **`timestamp_unix`**:
        *   **Data Type:** `Number` (Unsigned 64-bit integer)
        *   **Presence:** Guaranteed
        *   **Description:** The timestamp of the current block or transaction being processed by the runtime, represented as Unix Epoch seconds (seconds since 1970-01-01T00:00:00Z). This should be a securely sourced and monotonically increasing value.
        *   **Note:** CCL standard library will provide functions to format this as an RFC 3339 / ISO 8601 string (e.g., "2023-10-27T10:30:00Z") if needed for logging or external representation.
    *   **`block_height`**:
        *   **Data Type:** `Number` (Unsigned 64-bit integer)
        *   **Presence:** Guaranteed
        *   **Description:** The current block number or height in the ICN ledger.
    *   **`network_id`**:
        *   **Data Type:** `String`
        *   **Presence:** Guaranteed
        *   **Description:** A unique identifier for the specific ICN network or federation the contract is executing on (e.g., "icn-mainnet", "icn-devnet-alpha").
    *   **`transaction_id`**:
        *   **Data Type:** `String` (Typically a cryptographic hash)
        *   **Presence:** Optional
        *   **Description:** The unique identifier of the specific transaction or interaction that led to this CCL execution. `null` if not applicable for the execution trigger.
    *   **`execution_mode`**:
        *   **Data Type:** `String` (Enum: "Genesis", "Normal", "Replay", "Query". List to be formally defined in Canonical Types RFC.)
        *   **Presence:** Guaranteed
        *   **Description:** Indicates the mode in which the CCL is currently executing.

### 3.2. `caller` Namespace

*   **Purpose:** Provides information about the authenticated entity (user, service, or another contract) that initiated the current CCL execution flow.
*   **Properties:**
    *   **`id`**:
        *   **Data Type:** `DID` (String representing a Decentralized Identifier)
        *   **Presence:** Guaranteed
        *   **Description:** The DID of the immediate caller that invoked the action or process leading to this CCL execution.
    *   **`roles`**:
        *   **Data Type:** `Array<String>`
        *   **Presence:** Guaranteed (Returns an empty array `[]` if no roles are resolved or applicable, never `null`)
        *   **Description:** An array of resolved role strings for the `caller.id` relevant to the current contract's execution context (e.g., as determined by an active `roles_def`). CCL authors can check for role presence using `if "admin" in caller.roles then ...`.
    *   **`original_initiator_id`**:
        *   **Data Type:** `DID`
        *   **Presence:** Optional
        *   **Description:** In a chain of calls, this would be the DID of the very first entity that started the sequence. `null` if the current caller is the original initiator or if this information is not tracked.

### 3.3. `trigger` Namespace

*   **Purpose:** Provides information about the specific trigger that caused the current CCL contract logic to execute.
*   **Properties:**
    *   **`id`**:
        *   **Data Type:** `String` (Unique identifier, e.g., UUID, transaction hash part)
        *   **Presence:** Guaranteed
        *   **Description:** A unique identifier for this specific trigger instance.
    *   **`type`**:
        *   **Data Type:** `String` (Enum-like or dot.separated.string, e.g., "direct.call", "event.external.voting_closed")
        *   **Presence:** Guaranteed
        *   **Description:** Describes the nature of the trigger.
    *   **`source_uri`**:
        *   **Data Type:** `String` (URI)
        *   **Presence:** Optional
        *   **Description:** A URI identifying the origin or source of the trigger. `null` if not applicable.
    *   **`timestamp_unix`**:
        *   **Data Type:** `Number` (Unsigned 64-bit integer, Unix Epoch seconds)
        *   **Presence:** Guaranteed
        *   **Description:** The timestamp associated with when the trigger occurred or was recorded. Consistent with `system.timestamp_unix`.
    *   **`parameters`**:
        *   **Data Type:** `Object` (Key-value map, typically representing deserialized JSON or CBOR) or `null`.
        *   **Presence:** Guaranteed (The `parameters` key itself is always present; its value is an `Object` or `null`)
        *   **Description:** Contains the data or arguments associated with the trigger. For a direct call, these would be the function arguments. For an event, this would be the event's payload. Accessed via dot-notation, e.g., `trigger.parameters.amount`. Value is `null` if the trigger has no parameters/payload.

### 3.4. `contract` Namespace

*   **Purpose:** Provides information about the current CCL contract instance and its definition.
*   **Properties:**
    *   **`instance_id`**:
        *   **Data Type:** `String` (Unique identifier)
        *   **Presence:** Guaranteed
        *   **Description:** The unique identifier for this specific executing instance of the CCL contract.
    *   **`definition_id`**:
        *   **Data Type:** `String` (e.g., CID of the CCL source, or a registered name/version)
        *   **Presence:** Guaranteed
        *   **Description:** An identifier for the CCL definition (template) this instance is based on.
    *   **`current_stage_name`**:
        *   **Data Type:** `String`
        *   **Presence:** Optional (Applicable primarily for `process_def` instances)
        *   **Description:** If the contract is a stateful process with stages, this holds the name of the current active stage. `null` otherwise.
    *   **`status`**:
        *   **Data Type:** `String`
        *   **Presence:** Guaranteed
        *   **Description:** The current execution status of this contract instance (e.g., "Initializing", "Running", "Succeeded", "Failed").
    *   **`properties`**:
        *   **Data Type:** `Object` or `null`.
        *   **Presence:** Guaranteed (The `properties` key itself is always present; its value is an `Object` or `null`)
        *   **Description:** Provides read-only access to static properties defined within the current contract definition's scope (e.g., a top-level `proposal_def` field or a property of the currently executing `stage_def`). The runtime scopes this object to reflect relevant definition attributes for the current point of execution. For example, if inside a stage's `enter_action`, `contract.properties.duration` would refer to that stage's defined duration. Value is `null` if no such properties are defined or applicable for the current scope.

## 4. Access Semantics & Behavior

*   **Path Resolution:** Context variables are accessed using dot-notation (e.g., `caller.id`, `trigger.parameters.customer.name`).
*   **Missing Keys/Paths & Invalid Intermediate Paths:**
    *   If an attempt is made to access a property that does not exist at a valid path (e.g., `caller.non_existent_key`), the access **MUST** result in a `null` value.
    *   If an attempt is made to access a sub-property of a value that is `null` or not an `Object` (e.g., `trigger.parameters.customer.name` when `trigger.parameters.customer` is `null` or a string), the access **MUST** also result in `null`.
    *   In CCL, a context key (e.g., `trigger.parameters` or `contract.properties`) may be guaranteed to exist in the context object even if its value is `null`. This allows presence checks to remain syntactically simple (e.g., `if trigger.parameters != null then ...`).
*   **Type Safety & Error Handling:**
    *   The runtime and host ABI are responsible for providing data with the types specified in this model.
    *   CCL standard library **MUST** provide built-in functions for type checking (e.g., `is_string(value)`, `is_number(value)`, `is_object(value)`, `is_array(value)`).
    *   CCL standard library **MUST** provide built-in functions for explicit, safe type conversions (e.g., `try_to_string(value)`, `try_to_number(value)`), which return a result object indicating success/failure or the converted value/`null`.
    *   Attempting direct operations on mismatched types without explicit conversion (e.g., arithmetic on a string, adding a number to an object) **MUST** result in the CCL execution trapping (halting with an unrecoverable error). Implicit type coercion by operators (e.g., `+`, `-`, `*`, `/`, `==`, `>`) is forbidden to prevent unexpected behavior.

## 5. Extensibility

While these core namespaces provide a foundational context, specific CCL applications or host environments might introduce additional, specialized namespaces or properties. Such extensions should be clearly documented, avoid conflicts with core namespace definitions, and follow the same access semantics and type safety principles.

---
> **🛠️ Runtime Implementation Note:**
> Context namespaces are typically constructed at runtime by serializing environment data into a flat memory segment (e.g., as CBOR or JSON), with schema-aware deserialization provided by the CCL engine or standard library within the WASM module. Future extensions (e.g., context compression, host-backed lazy access for certain properties) are permitted if they preserve the access semantics and type model defined in this RFC. The exact method of passing this initial context (e.g., via a designated memory region or an initialization function call) will be detailed in the Host ABI specification.
--- 