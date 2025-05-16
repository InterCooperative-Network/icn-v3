# ICN Host ABI Specification (Draft)

## 1. Introduction & Overview

### 1.1. Purpose of this Document
This document specifies the Application Binary Interface (ABI) between WebAssembly (WASM) modules compiled from ICN Contract Chain Language (CCL) and the ICN host execution environment (`icn_host`). It defines the imported host functions, data passing conventions, expected behaviors, error handling, and core schemas that govern this interaction.

### 1.2. Scope
The scope of this ABI is the direct interface between the compiled WASM contract and the immediate host functions it can invoke. It covers the operational semantics of these functions and the data structures they operate upon.

### 1.3. Core Principles
The ICN Host ABI is designed around the following core principles:
* **Stateful Context Management:** The host maintains a stack of active contexts (e.g., proposal, section, event handler, conditional states) to correctly scope operations.
* **Schema-Driven Validation:** The host actively validates incoming data (keys, values, kinds, event names, job parameters, etc.) against predefined schemas, acting as a guardian of structural and semantic integrity.
* **Explicit Data Passing:** String and complex data are passed from WASM to host primarily via `(ptr, len)` pairs referring to data in WASM linear memory, often as well-defined (e.g., JSON) serializations.
* **Clear Error Handling:** Specific, documented error codes are used to signal ABI violations and operational failures, leading to halted execution.
* **Atomic Operations:** Critical state changes (e.g., token transfers, resource debits, job submissions) are designed to be atomic.
* **Explicit Authorization & Permissions:** Secure operation relies on checking permissions and capabilities for significant actions.
* **Defined Expression Language:** A simple, secure language for evaluating conditions in conditional opcodes.
* **Resource Accounting:** A per-execution-context budget model for managing computational and other resources.

### 1.4. Versioning of the ABI
This document pertains to version X.Y.Z of the ICN Host ABI. Future versions will be versioned accordingly.

## 2. Host Interaction Model

### 2.1. Opcode Stream Processing
The WASM module, generated from CCL, executes a sequence of high-level Opcodes. Most Opcodes translate directly or indirectly into calls to imported host functions. The host processes these calls sequentially, maintaining state.

### 2.2. Host Context Stack
The host utilizes a context stack to manage the scope of operations. Contexts can include:
* `ProposalContext`: Initiated by `host_create_proposal`.
* `SectionContext`: Initiated by `host_begin_section`.
* `EventHandlerContext`: Initiated by `host_on_event`.
* `ConditionalExecutionContext`: Internal state managed by `host_if`, `host_else`, `host_end_if`.

Each context may hold its own properties and nested elements.

### 2.3. Data Passing Conventions
* **Strings (including JSON strings):** Passed as `(ptr: i32, len: i32)` pairs from WASM, where `ptr` is the starting offset in WASM linear memory and `len` is the byte length of the UTF-8 encoded string. The host reads these bytes.
* **Numbers:** Passed directly as `i32`, `i64`, `f64` on the WASM stack.
* **Booleans:** Often implicit or passed as `i32` (0 or 1) if needed directly on stack.

### 2.4. Error Handling Philosophy & Error Code System
* Upon encountering a validation error or operational failure during the execution of a host function, the host MUST halt further execution of the current logical operation or transaction.
* The host maintains an internal last-error state. This state might be queryable by an external environment supervising the host, but error conditions are not typically returned as values onto the WASM stack to continue WASM execution (unless specified by a particular host function's return type, e.g., `host_submit_mesh_job`).
* Error codes are strings (e.g., `ERR_ABI_MEMORY_READ_FAILURE`). A full list will be in Appendix A.

    #### 2.4.1. General ABI Error Codes
    * `ERR_ABI_MEMORY_READ_FAILURE`: Failure to read data from WASM linear memory based on ptr/len.
    * `ERR_UNKNOWN_HOST_FUNCTION_INDEX`: An invalid function index was called.

    #### 2.4.2. Opcode-Specific Error Code Ranges/Prefixes
    (To be detailed further; e.g., `ERR_PROPOSAL_*`, `ERR_SECTION_*`)

### 2.5. Resource Accounting Model
(To be detailed based on `Opcode::UseResource` and `Opcode::SubmitJob` specs - e.g., Per-Execution Context Budget).

### 2.6. Permissions and Capabilities Model
(To be detailed - conceptual for now, outlining how permissions might restrict opcode/host function usage).

## 3. Imported Host Functions (`"icn_host"`)

### 3.1. Summary Table
(This table will be populated as we detail each function. Example entry:)
| Index | Name                 | WASM Signature                                        | Brief Description                     |
|-------|----------------------|-------------------------------------------------------|---------------------------------------|
| 0     | `begin_section`      | `(i32, i32, i32, i32) -> ()`                          | Begins a new typed and titled section |
| 1     | `end_section`        | `() -> ()`                                            | Ends the current section              |
| ...   | ...                  | ...                                                   | ...                                   |
| 16    | `host_submit_mesh_job`| `(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i64, i32) -> i32`      | Submits a job to the mesh network     |

### 3.2. Detailed Specification for Each Host Function

---
#### **Host Function 0: `begin_section`**

* **WASM Signature:**
    `(param "kind_ptr" i32) (param "kind_len" i32) (param "title_ptr" i32) (param "title_len" i32) -> ()`
* **Corresponding Opcode(s):** Primarily triggered by `Opcode::BeginSection { kind: String, title: Option<String> }`. Also used internally by the `WasmGenerator` when processing `RuleValue::Range` (from `range_statement` or `range_value` in `lower.rs`), where `kind` will be a generated string like `"range_START_END"` and `title` will be the `Rule.key` associated with that range.
* **Argument Interpretation:**
    * `kind_ptr`, `kind_len`: Host MUST read the UTF-8 string for the section `kind` from WASM linear memory.
    * `title_ptr`, `title_len`: Host MUST read the UTF-8 string for the section `title` from WASM linear memory. If `title_len` is 0, the title is considered absent (`None`).
* **Host State Modification:**
    1.  Upon invocation, the host MUST create a new internal "section context" object.
    2.  This new section context MUST be pushed onto the host's main context stack, becoming the "current active context."
    3.  If the stack was empty, this section is a top-level section. (Schema will define if this is allowed for the given kind).
    4.  If the stack was not empty, this section is a child of the context previously at the top of the stack. The new section context SHOULD maintain a reference to its parent context.
    5.  The section context MUST store the resolved `kind` and `title` (if provided and `title_len > 0`).
    6.  It MUST be prepared to accumulate properties (from `host_set_property`), ordered child sections (from nested `host_begin_section` calls), and other nested structures.
    7.  Internal Section ID: No persistent unique ID is mandated by the ABI for section contexts. Their identity is defined by `kind`, `title`, and path/position. The host MAY assign transient internal identifiers.
* **Validation Logic:**
    1.  **Schema Enforcement:** The host SHOULD operate with a predefined "structural schema" defining allowed `kind` strings, permissible parent `kind`(s), allowed child section `kind`(s) and their cardinality, and expected properties for each `kind`.
    2.  If schema enforced:
        * The provided `kind` string MUST be recognized. Error: `ERR_SECTION_KIND_UNKNOWN`.
        * Nesting (parent's `kind`) MUST be valid according to the schema. Error: `ERR_SECTION_INVALID_PARENT_FOR_KIND`.
    3.  **`kind` String:**
        * MUST NOT be empty. Error: `ERR_SECTION_KIND_EMPTY`.
        * SHOULD have a host-enforced maximum length (e.g., 128 characters). Error: `ERR_SECTION_KIND_TOO_LONG`.
        * SHOULD adhere to a defined format (e.g., snake_case, `[a-zA-Z0-9_.-]+`). Error: `ERR_SECTION_KIND_INVALID_FORMAT`.
    4.  **`title` String:**
        * If `title_len > 0` (indicating a title is provided): the resolved title string MUST NOT be empty (e.g. `""`). If it is, `ERR_SECTION_TITLE_EMPTY` MUST be raised.
        * SHOULD have a maximum length (e.g., 256-1024 characters). Error: `ERR_SECTION_TITLE_TOO_LONG`.
        * SHOULD adhere to character set rules (e.g., printable UTF-8). Error: `ERR_SECTION_TITLE_INVALID_CHARS`.
    5.  **Stack Depth:** The host MAY enforce a maximum nesting depth for sections. Error: `ERR_MAX_SECTION_DEPTH_REACHED`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution and the host setting an internal last-error state.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_SECTION_KIND_UNKNOWN`
    * `ERR_SECTION_INVALID_PARENT_FOR_KIND`
    * `ERR_SECTION_KIND_EMPTY`
    * `ERR_SECTION_KIND_TOO_LONG`
    * `ERR_SECTION_KIND_INVALID_FORMAT`
    * `ERR_SECTION_TITLE_EMPTY`
    * `ERR_SECTION_TITLE_TOO_LONG`
    * `ERR_SECTION_TITLE_INVALID_CHARS`
    * `ERR_MAX_SECTION_DEPTH_REACHED`
* **Interaction with Other Host Functions:**
    * Initiates a context that `host_set_property`, `host_call_host`, nested `host_begin_section`, and `host_if` will operate within.
    * Must be balanced by a `host_end_section` call.

---
#### **Host Function 1: `end_section`**

* **WASM Signature:**
    `() -> ()`
* **Corresponding Opcode(s):** Triggered by `Opcode::EndSection`.
* **Argument Interpretation:** None.
* **Host State Modification:**
    1.  The host MUST verify that the current active context on its context stack is indeed a "section context" (i.e., was initiated by `host_begin_section`).
    2.  **Finalization Actions (Schema-Driven):** Before popping, the host SHOULD perform final validation on the section context being closed, based on its `kind` and the host's structural schema:
        * **Required Properties**: Check if all mandatory properties for this section `kind` have been set.
        * **Property Values**: Perform final type/constraint validation on collected properties.
        * **Required Child Sections/Elements**: Check for presence of mandatory child sections or elements.
        * **Cardinality of Child Sections**: Validate number of child sections of specific `kind`s.
    3.  If all finalization validations pass:
        * The finalized section object (containing its `kind`, `title`, validated properties, and nested child objects) is considered complete. This object should be immutable from the perspective of its parent once finalized.
        * This completed section object MUST be "attached" as a child element to its parent context.
    4.  The current section context is then popped from the host's context stack.
    5.  The parent context (if any) becomes the new "current active context." If the stack becomes empty, the host returns to a base processing state.
* **Validation Logic (Pre-Pop & During Finalization):**
    1.  Context stack MUST NOT be empty.
    2.  Context at the top of the stack MUST have been initiated by `host_begin_section`.
    3.  All schema-defined finalization checks for the section's `kind` MUST pass.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_SECTION_STACK_EMPTY_ON_END`
    * `ERR_SECTION_MISMATCH_ON_END`
    * `ERR_SECTION_MISSING_REQUIRED_PROPERTY` (Host MAY include key name in internal error state)
    * `ERR_SECTION_INVALID_PROPERTY_VALUE`
    * `ERR_SECTION_MISSING_REQUIRED_CHILD_SECTION` (Host MAY include child kind in internal error state)
    * `ERR_SECTION_INVALID_CHILD_SECTION_CARDINALITY`
    * `ERR_SECTION_SCHEMA_VALIDATION_FAILED` (generic for other schema violations)
* **Interaction with Other Host Functions:**
    * Closes the scope opened by the most recent matching `host_begin_section`.
    * Relies on data populated within its scope for finalization checks.
    * Subsequent host function calls apply to the restored parent context.

---
#### **Host Function 2: `set_property`**

* **WASM Signature (from `emit.rs` type index 9):**
    `(param "key_ptr" i32) (param "key_len" i32) (param "value_json_ptr" i32) (param "value_json_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::SetProperty { key: String, value_json: String }`.
* **Argument Interpretation:**
    * `key_ptr`, `key_len`: Host MUST read the UTF-8 string for the property `key` from WASM linear memory.
    * `value_json_ptr`, `value_json_len`: Host MUST read the UTF-8 string representing the property `value_json` from WASM linear memory.
* **Host State Modification:**
    1.  `host_set_property` MUST operate within an active context (e.g., a proposal context or a section context) that has been pushed onto the host's context stack by `host_create_proposal` or `host_begin_section`.
    2.  The host MUST first attempt to parse the `value_json` string into a generic internal JSON value representation (e.g., `serde_json::Value` or equivalent). If this initial parsing fails (i.e., the string is not well-formed JSON), `ERR_SET_PROPERTY_INVALID_JSON_PAYLOAD` MUST be raised. The successfully parsed generic JSON value is then subject to further schema-based validation.
    3.  Upon successful validation (see Validation Logic), the host MUST add/associate the resolved `key` and the parsed, validated internal representation of `value_json` to the current active context's collection of properties.
* **Validation Logic (Highly dependent on the Host Schema):**
    1.  **Active Context:** The host MUST verify that there is an active context on the stack. If not, `ERR_SET_PROPERTY_NO_ACTIVE_CONTEXT` MUST be raised.
    2.  **`key` String Validation:**
        * The resolved `key` string MUST NOT be empty. Error: `ERR_SET_PROPERTY_KEY_EMPTY`.
        * The `key` string SHOULD adhere to defined format constraints (e.g., max length, character set like `[a-zA-Z0-9_.-]+`). Error: `ERR_SET_PROPERTY_KEY_INVALID_FORMAT`.
        * **Schema-based Key Recognition:** The host SHOULD validate that the `key` is a recognized and permitted property name for the `kind` of the current active context (e.g., proposal, section of kind "role_attributes"), according to the host's structural schema. Error: `ERR_SET_PROPERTY_KEY_UNKNOWN_FOR_CONTEXT`.
        * **Duplicate Key Handling:**
            * By default, the host MUST REJECT `host_set_property` if the `key` already exists within the property set of the current active context. Error: `ERR_SET_PROPERTY_DUPLICATE_KEY`.
            * *Exception*: If the host's structural schema for the current context `kind` explicitly defines that a particular `key` can be overwritten or can appear multiple times, the host MAY follow that schema-defined behavior for that specific key only. Such exceptions must be clearly documented in the schema.
    3.  **`value_json` Parsing and Validation (Post initial successful JSON parse):**
        * **Schema-based Value Validation (Layered Approach):**
            * **Level 1 (Basic Type Validation - MUST):** The host MUST validate the basic JSON type (string, number, boolean, array, object, null) of the parsed `value_json` against the type expected by the host's structural schema for the given `key` in the current context `kind`. If mismatched, `ERR_SET_PROPERTY_TYPE_MISMATCH` MUST be raised.
            * **Level 2 (Structural Validation - SHOULD):** If the schema defines expected nested fields for objects or item types/schemas for arrays, the host SHOULD validate these. This includes checking for the presence of mandatory nested keys and their basic types. Errors here could be `ERR_SET_PROPERTY_VALUE_SCHEMA_VIOLATION`.
            * **Level 3 (Content Constraint Validation - STRIVE FOR):** If the schema for the `key` defines more specific content constraints (e.g., number ranges, string patterns, enum values, array min/max length, specific item constraints), the host SHOULD validate the `value_json` against them. Errors here would also be `ERR_SET_PROPERTY_VALUE_SCHEMA_VIOLATION`.
    4.  **Immutable Property Handling:** The host's structural schema MAY define certain properties (for a given context `kind` and `key`) as immutable or "set-once." If `host_set_property` attempts to modify such a property that has already been set and is marked immutable, `ERR_SET_PROPERTY_CANNOT_MODIFY_IMMUTABLE` MUST be raised.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_SET_PROPERTY_NO_ACTIVE_CONTEXT`
    * `ERR_SET_PROPERTY_KEY_EMPTY`
    * `ERR_SET_PROPERTY_KEY_INVALID_FORMAT`
    * `ERR_SET_PROPERTY_KEY_UNKNOWN_FOR_CONTEXT`
    * `ERR_SET_PROPERTY_DUPLICATE_KEY`
    * `ERR_SET_PROPERTY_CANNOT_MODIFY_IMMUTABLE`
    * `ERR_SET_PROPERTY_INVALID_JSON_PAYLOAD`
    * `ERR_SET_PROPERTY_TYPE_MISMATCH`
    * `ERR_SET_PROPERTY_VALUE_SCHEMA_VIOLATION` (Host logs SHOULD provide more specific details for this error when possible)
* **Interaction with Other Host Functions:**
    * `host_set_property` is meaningful only when an appropriate context has been established by `host_create_proposal` or `host_begin_section`.
    * It populates the data within the current active context.
    * The properties set by this function are critical for the validation logic in `host_end_section`, for the condition evaluation by the host's internal condition evaluator for `host_if`, and may be read or used by host functions invoked via `host_call_host`.

---
#### **Host Function 3: `anchor_data`**

* **WASM Signature (from `emit.rs` type index 4):**
    `(param "path_ptr" i32) (param "path_len" i32) (param "data_ref_ptr" i32) (param "data_ref_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::AnchorData { path: String, data_ref: String }`.
* **Argument Interpretation:**
    * `path_ptr`, `path_len`: Host MUST read the UTF-8 string for the anchor `path` from WASM linear memory.
    * `data_ref_ptr`, `data_ref_len`: Host MUST read the UTF-8 string for the `data_ref` from WASM linear memory.
* **Host State Modification:**
    1.  `host_anchor_data` instructs the host to create an immutable, verifiable association between the given `path` and the interpreted `data_ref`. This operation MUST be write-once for a given `path` within its scope.
    2.  The host MUST store this `(path, canonical_data_reference)` pair in a persistent manner suitable for immutable records (e.g., within a ledger's state tree, a dedicated anchoring data structure).
    3.  **Contextual Association:** The anchor SHOULD be associated with the current active context (e.g., proposal, section) if one exists. The host determines if the `path` is considered global or relative to the current context. The host's structural schema MAY define where anchors can be placed.
* **Data Handling (Interpretation of `data_ref`):**
    The host receives `data_ref` as a string. It MUST be processed according to the following strategy to determine the `canonical_data_reference` to be stored with the `path`:
    1.  **Attempt 1: Parse as CID (Content Identifier):**
        * The host SHOULD first attempt to parse the `data_ref` string as a standard Content Identifier (e.g., validating its multihash, codec, and base encoding).
        * If successful and valid, this CID is the `canonical_data_reference`.
    2.  **Attempt 2: Parse as JSON string and compute CID:**
        * If `data_ref` is not recognized as a valid CID in Attempt 1, the host SHOULD attempt to parse it as a JSON string.
        * If successful (it's valid JSON):
            * The host MUST convert the parsed JSON object/value into a canonical binary representation (e.g., RFC 8785 JSON Canonicalization, or canonical CBOR as per IPLD specifications).
            * The host MUST then compute a CID of this canonical binary representation using a host-defined, well-specified hashing algorithm and encoding (e.g., SHA2-256 multihash, Base32 CIDv1). This computed CID becomes the `canonical_data_reference`.
            * The host MAY cache the canonical binary form associated with this computed CID for efficient retrieval or to ensure it's available to the underlying content-addressed storage layer. The host is NOT REQUIRED by this opcode to ensure the data for this computed CID is *globally discoverable* (e.g., on IPFS) at the time of anchoring, but it's responsible for the integrity of the `path` -> `computed_CID` mapping.
    3.  **Attempt 3: Fallback to Opaque String (Schema-Permitting Only):**
        * If `data_ref` is not a recognized CID (Attempt 1) and not valid JSON (Attempt 2), it is considered an opaque string.
        * This opaque string can only be used as the `canonical_data_reference` if the host's structural schema, for the current context or `path` prefix, explicitly permits "opaque" or "non-content-addressed" references.
        * If opaque strings are not permitted by the schema for this context, this case MUST result in an error (e.g., `ERR_ANCHOR_DATA_REF_INVALID_FORMAT`).
* **Validation Logic:**
    1.  **Active Context:** An active context (e.g., proposal, section) SHOULD be required for most anchoring operations, unless the host's schema defines specific global paths where context-less anchoring is permitted. If required and none is active, `ERR_ANCHOR_NO_ACTIVE_CONTEXT` MUST be raised.
    2.  **`path` String Validation:**
        * MUST NOT be empty. Error: `ERR_ANCHOR_PATH_EMPTY`.
        * MUST adhere to a host-defined path syntax (e.g., `/` delimited, restricted character set like `[a-zA-Z0-9_./-]+`, max length, max segment count). Error: `ERR_ANCHOR_PATH_INVALID_FORMAT`.
        * **Uniqueness/Immutability**: A specific `path` (within its defined scope, e.g., global or relative to current context) MUST only be anchored once. Attempting to re-anchor an existing `path` MUST result in `ERR_ANCHOR_PATH_ALREADY_ANCHORED`.
        * The host MAY have reserved path prefixes. Error: `ERR_ANCHOR_PATH_RESERVED`.
    3.  **`data_ref` String and Interpreted `canonical_data_reference` Validation:**
        * The raw `data_ref` string read from memory MUST NOT be empty. Error: `ERR_ANCHOR_DATA_REF_EMPTY`.
        * If Attempt 1 (Parse as CID) is pursued: The string MUST be a structurally valid CID. Error: `ERR_ANCHOR_DATA_REF_INVALID_CID`. (Note: This opcode does not mandate that the host *resolves* the CID's data, only that the reference itself is valid).
        * If Attempt 2 (Parse as JSON) is pursued: The string MUST be well-formed JSON. Error: `ERR_ANCHOR_DATA_REF_INVALID_JSON`. The subsequent canonicalization and CID computation must succeed (else `ERR_ANCHOR_DATA_REF_CANONICALIZATION_FAILED`).
        * If Attempt 3 (Fallback to Opaque String) is pursued: This fallback MUST be permitted by the host's schema for the given context/path. Error: `ERR_ANCHOR_DATA_REF_OPAQUE_NOT_ALLOWED`.
        * The host's schema MAY define further constraints on the *content* of a `data_ref` if it's JSON (e.g., requiring specific metadata fields, max size). Error: `ERR_ANCHOR_DATA_JSON_SCHEMA_VIOLATION`.
    4.  **Permissions/Capabilities:** The current contract execution context MUST have permission to anchor data, potentially restricted by `path` prefixes or the nature of the `data_ref`. Error: `ERR_ANCHOR_PERMISSION_DENIED`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_ANCHOR_NO_ACTIVE_CONTEXT`
    * `ERR_ANCHOR_PATH_EMPTY`
    * `ERR_ANCHOR_PATH_INVALID_FORMAT`
    * `ERR_ANCHOR_PATH_ALREADY_ANCHORED`
    * `ERR_ANCHOR_PATH_RESERVED`
    * `ERR_ANCHOR_DATA_REF_EMPTY`
    * `ERR_ANCHOR_DATA_REF_INVALID_CID`
    * `ERR_ANCHOR_DATA_REF_INVALID_JSON`
    * `ERR_ANCHOR_DATA_REF_CANONICALIZATION_FAILED`
    * `ERR_ANCHOR_DATA_REF_OPAQUE_NOT_ALLOWED`
    * `ERR_ANCHOR_DATA_JSON_SCHEMA_VIOLATION`
    * `ERR_ANCHOR_PERMISSION_DENIED`
    * `ERR_ANCHOR_OPERATION_FAILED` (generic failure during storage/recording of the anchor)
* **Interaction with Other Host Functions:**
    * `host_anchor_data` creates an immutable record within the host's state.
    * This record (the association between `path` and `canonical_data_reference`) might be queryable via other host functions invoked by `host_call_host` (e.g., `get_anchor_reference(path)`).
    * The existence or content of an anchor could be used in conditions evaluated by `host_if` (if the condition language can query anchor states).

---
#### **Host Function 4: `generic_call`**

* **WASM Signature (from `emit.rs` type index 5):**
    `(param "fn_name_ptr" i32) (param "fn_name_len" i32) (param "args_payload_ptr" i32) (param "args_payload_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::CallHost { fn_name: String, args_payload: String }`.
* **Argument Interpretation:**
    * `fn_name_ptr`, `fn_name_len`: Host MUST read the UTF-8 string for the target `fn_name` (host function name) from WASM linear memory.
    * `args_payload_ptr`, `args_payload_len`: Host MUST read the UTF-8 string representing the `args_payload` (a JSON string containing the arguments for `fn_name`) from WASM linear memory.
* **Host State Modification:**
    1.  `host_generic_call` itself is primarily a dispatcher. The actual state modification is performed by the specific underlying host function identified by `fn_name`.
    2.  The underlying host function MAY be executed globally or within an active context (e.g., proposal, section), as defined by its individual schema (see Validation Logic).
    3.  The execution of the dispatched host function MAY modify the host's internal state, the current active context (if any, e.g., by setting properties), create new host-managed entities, or emit host-level events. These side effects MUST be documented in the schema for each specific `fn_name`.
    4.  Since this `host_generic_call` function has a `-> ()` signature (no direct return value to the WASM stack), any "results" from the dispatched host function must be communicated via side effects on the host state or by the dispatched function internally setting properties on the active context.
* **Data Handling:**
    1.  The host MUST successfully read the `fn_name` string.
    2.  The host MUST successfully read the `args_payload` string.
    3.  The host MUST parse the `args_payload` string as JSON (e.g., into an internal generic JSON value representation like `serde_json::Value`). Failure to parse is an error (`ERR_CALL_HOST_INVALID_ARGS_JSON_PAYLOAD`).
* **Validation Logic (Dispatcher and Per-Function Schema):**
    1.  **`fn_name` Recognition & Dispatch:**
        * The host MUST maintain a central registry of all known, invokable host functions accessible via `host_generic_call`.
        * Each registered function in this registry MUST have an associated schema defining its specific behavior, including:
            * Expected `argument_schema` (JSON Schema or equivalent for the *parsed JSON content* of `args_payload`, detailing argument names, types, optionality, constraints).
            * `context_requirements` (whether an active context is needed, and if so, permissible `kind`s).
            * `permissions_required` (capabilities needed to invoke this function).
            * Detailed `side_effects_description`.
        * The `fn_name` string read from WASM MUST exist as a key in this host function registry. If not, `ERR_CALL_HOST_UNKNOWN_FUNCTION_NAME` MUST be raised.
    2.  **Active Context Check (as per `fn_name`'s schema):**
        * If the schema for the resolved `fn_name` requires an active data context, the host MUST verify one exists and its `kind` is permissible. Errors: `ERR_CALL_HOST_NO_ACTIVE_CONTEXT`, `ERR_CALL_HOST_INVALID_CONTEXT_KIND`.
    3.  **Permissions/Capabilities Check (as per `fn_name`'s schema):**
        * The host MUST verify that the current contract execution context possesses the `permissions_required` specified in the schema for the resolved `fn_name`. Error: `ERR_CALL_HOST_PERMISSION_DENIED`.
    4.  **Argument Deserialization & Validation (as per `fn_name`'s `argument_schema`):**
        * Using the `argument_schema` associated with the resolved `fn_name`, the host MUST attempt to deserialize and validate the parsed `args_payload` (the generic JSON value) into the specific argument structure expected by the underlying host function.
        * All arguments marked as "required" in the `argument_schema` MUST be present and correctly typed in the `args_payload`.
        * The types and values of all provided arguments MUST conform to the types and constraints (patterns, ranges, enums, nested field schemas, etc.) defined in the `argument_schema`. This implies Level 1, 2, and 3 validation as discussed for `host_set_property`, applied here per function. Errors: `ERR_CALL_HOST_ARGS_SCHEMA_VIOLATION` (for structural issues like missing required args or basic type mismatches), `ERR_CALL_HOST_INVALID_ARGUMENT_VALUE` (for constraint violations).
    5.  **Contextual Appropriateness (as per `fn_name`'s schema):**
        * The schema for `fn_name` MAY specify further conditions on the state of the active context for the function to be callable. Error: `ERR_CALL_HOST_FUNCTION_NOT_APPLICABLE_IN_STATE`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution. Success implies the dispatched host function was invoked; its own success/failure might be signaled via side effects or subsequent errors if it encounters issues not caught by argument validation.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_CALL_HOST_NO_ACTIVE_CONTEXT`
    * `ERR_CALL_HOST_INVALID_CONTEXT_KIND`
    * `ERR_CALL_HOST_UNKNOWN_FUNCTION_NAME`
    * `ERR_CALL_HOST_INVALID_ARGS_JSON_PAYLOAD`
    * `ERR_CALL_HOST_ARGS_SCHEMA_VIOLATION`
    * `ERR_CALL_HOST_INVALID_ARGUMENT_VALUE`
    * `ERR_CALL_HOST_PERMISSION_DENIED`
    * `ERR_CALL_HOST_FUNCTION_NOT_APPLICABLE_IN_STATE`
    * Any specific error codes raised by the *execution* of the dispatched host function itself (e.g., `ERR_HOST_FUNCTION_EXECUTION_FAILED_{SPECIFIC_ERROR_CODE}`). These should ideally be documented per registered `fn_name`, and upon such an error, `host_generic_call` MUST ensure overall execution halts signaling this specific error.
* **Interaction with Other Host Functions:**
    * `host_generic_call` acts as a gateway to a wide array of host-provided functionalities.
    * The dispatched host function may interact with the current active context (reading properties via internal host APIs, calling `host_set_property` equivalent logic internally to update it), create new contexts, or affect global host state.
    * Its execution can significantly influence subsequent operations and condition evaluations.

---
#### **Host Function 5: `create_proposal`**

* **WASM Signature (Revised - from `emit.rs` type index 2, updated for `id`):**
    `(param "id_ptr" i32) (param "id_len" i32) (param "title_ptr" i32) (param "title_len" i32) (param "version_ptr" i32) (param "version_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::CreateProposal { id: String, title: String, version: String }`.
* **Argument Interpretation:**
    * `id_ptr`, `id_len`: Host MUST read the UTF-8 string for the proposal `id` (expected to be a UUID string) from WASM linear memory.
    * `title_ptr`, `title_len`: Host MUST read the UTF-8 string for the proposal `title` from WASM linear memory.
    * `version_ptr`, `version_len`: Host MUST read the UTF-8 string for the proposal `version` from WASM linear memory.
* **Host State Modification (as per our refined spec for `Opcode::CreateProposal`):**
    1.  Upon invocation, the host MUST create a new internal "proposal context" object.
    2.  The host SHOULD maintain a generic "context stack." The new proposal context becomes the top element of this stack.
    3.  If the host's context stack was not empty when `host_create_proposal` is invoked and the top context was also a proposal context (or another top-level module type that shouldn't be nested), the host should first implicitly finalize the previous context before creating and pushing the new one.
    4.  The new proposal context becomes the "current active context."
* **Internal Identification of Proposal Context:**
    * The resolved `id` (UUID string) MUST be used as the primary unique identifier for the proposal context within the host during its lifetime and for any persistence or cross-execution referencing.
* **Data Handling:**
    1.  The host MUST store the resolved `id`, `title`, and `version` strings as core, immutable attributes of the newly created proposal context.
    2.  The proposal context MUST be prepared to store a collection of associated data (properties, nested sections, action records, etc.) that will be defined by subsequent host function calls.
* **Validation Logic:**
    1.  **State Validity:** The host should ensure it's in a valid state to begin a new proposal (e.g., managing implicit finalization of any previous top-level module on the context stack).
    2.  **`id` (UUID String) Validation:**
        * MUST NOT be empty. Error: `ERR_PROPOSAL_ID_EMPTY`.
        * MUST be a valid UUID string representation. Error: `ERR_PROPOSAL_ID_INVALID_FORMAT`.
        * Host MAY (if a persistence layer exists and requires it) check for `id` uniqueness across already finalized proposals within a relevant scope. Error: `ERR_PROPOSAL_ID_DUPLICATE`.
    3.  **`title` String Validation:**
        * MUST NOT be empty. Error: `ERR_PROPOSAL_TITLE_EMPTY`.
        * SHOULD have a host-enforced maximum length (e.g., 256-1024 characters). Error: `ERR_PROPOSAL_TITLE_TOO_LONG`.
        * SHOULD consist of printable UTF-8 characters; specific control characters (other than common whitespace) MAY be disallowed. Error: `ERR_PROPOSAL_TITLE_INVALID_CHARS`.
    4.  **`version` String Validation:**
        * MUST NOT be empty. Error: `ERR_PROPOSAL_VERSION_EMPTY`.
        * SHOULD have a host-enforced maximum length (e.g., 64-128 characters). Error: `ERR_PROPOSAL_VERSION_TOO_LONG`.
        * SHOULD be validated against a Semantic Versioning (SemVer 2.0.0) pattern. Error: `ERR_PROPOSAL_VERSION_INVALID_FORMAT`.
    5.  **Resource Limits:** The host MAY enforce limits on the number of proposal definitions or their total data size per contract execution. Errors: `ERR_MAX_PROPOSALS_REACHED`, `ERR_PROPOSAL_DATA_SIZE_LIMIT_EXCEEDED`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_PROPOSAL_ID_EMPTY`
    * `ERR_PROPOSAL_ID_INVALID_FORMAT`
    * `ERR_PROPOSAL_ID_DUPLICATE`
    * `ERR_PROPOSAL_TITLE_EMPTY`
    * `ERR_PROPOSAL_TITLE_TOO_LONG`
    * `ERR_PROPOSAL_TITLE_INVALID_CHARS`
    * `ERR_PROPOSAL_VERSION_EMPTY`
    * `ERR_PROPOSAL_VERSION_TOO_LONG`
    * `ERR_PROPOSAL_VERSION_INVALID_FORMAT`
    * `ERR_MAX_PROPOSALS_REACHED`
    * `ERR_PROPOSAL_DATA_SIZE_LIMIT_EXCEEDED`
    * `ERR_INVALID_STATE_FOR_PROPOSAL` (if received when a proposal cannot begin)
* **Interaction with Other Host Functions:**
    * `host_create_proposal` initiates the active proposal context.
    * Subsequent `host_set_property`, `host_call_host`, `host_begin_section`, and `host_if` calls apply to this active proposal context.
    * The proposal context is considered "active" until implicitly finalized by another top-level module-initiating host function call (e.g., another `host_create_proposal`, `host_on_event`) or by the end of the WASM program execution. Upon finalization, the host performs any aggregate validation on the complete proposal object.

---
#### **Host Function 6: `mint_token`**

* **WASM Signature (from `emit.rs` type index 3, needs verification based on our string refactor):**
    Previously `(res_type_ptr, res_type_len, amount: i64, recip_ptr, recip_len, data_json_ptr, data_json_len) -> ()`.
    This signature is correct based on our `(ptr, len)` string refactor. Note `amount` is `i64` which matches `Opcode::MintToken`'s `u64` after WASM's `i64.const`.
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::MintToken { res_type: String, amount: u64, recipient: Option<String>, data: Option<String> /* JSON string */ }`.
* **Argument Interpretation:**
    * `res_type_ptr`, `res_type_len`: Host MUST read the UTF-8 string for the `res_type` (resource/token type) from WASM linear memory.
    * `amount`: Received directly as an `i64` (effectively `u64`) from the WASM stack.
    * `recip_ptr`, `recip_len`: Host MUST read the UTF-8 string for the `recipient` from WASM linear memory. If `recip_len` is 0, this indicates an absent recipient (`None`), and the host's default recipient logic for the `res_type` applies.
    * `data_json_ptr`, `data_json_len`: Host MUST read the UTF-8 string representing the `data` (a JSON string for metadata or specific instructions) from WASM linear memory. If `data_json_len` is 0, this indicates absent `data` (`None`).
* **Host State Modification (as per our spec for `Opcode::MintToken`):**
    1.  This is an action that directly modifies the host's ledger state by creating new tokens/assets.
    2.  The host MUST identify or ensure the existence of a ledger associated with the resolved `res_type`.
    3.  The specified `amount` of tokens of `res_type` MUST be credited to the resolved `recipient`.
        * If `recipient` is absent (indicated by `recip_len == 0`), the host MUST apply its schema-defined default recipient logic for the given `res_type` (e.g., credit to the contract instance, a treasury, or error if no default is defined and a recipient is mandatory).
    4.  If `data` is present (indicated by `data_json_len > 0`), the parsed JSON data MAY be stored in the ledger entry or associated with the minted token instances, according to the schema for `res_type`.
    5.  The entire minting operation (validation, ledger updates) MUST be atomic.
* **Data Handling:**
    1.  The host MUST parse `data_json_string` (if present) into an internal JSON value representation.
* **Validation Logic (Highly dependent on Host Schema for `res_type`):**
    1.  **Active Context:** The host MAY require an active context (e.g., `EventHandlerContext`, `ProposalContext`) for authorization, logging, or associating the minting event, as defined by the host's operational policies.
    2.  **`res_type` Validation:**
        * Resolved `res_type` string MUST NOT be empty. Error: `ERR_MINT_TOKEN_RES_TYPE_EMPTY`.
        * MUST be a recognized and "mintable" `res_type` defined in the host's resource/asset schema. Error: `ERR_MINT_TOKEN_RES_TYPE_UNKNOWN`.
        * The schema for `res_type` should define its properties (divisibility, max supply, fungibility, etc.).
        * Host SHOULD validate format (max length, char set). Error: `ERR_MINT_TOKEN_RES_TYPE_INVALID_FORMAT`.
        * The `res_type` must be flagged as mintable in its schema. Error: `ERR_MINT_TOKEN_RES_TYPE_NOT_MINTABLE`.
    3.  **`amount` Validation:**
        * MUST be greater than 0. Error: `ERR_MINT_TOKEN_AMOUNT_ZERO`.
        * MUST be validated against divisibility rules and potential maximum supply for `res_type` (from schema). Errors: `ERR_MINT_TOKEN_AMOUNT_INVALID_DIVISIBILITY`, `ERR_MINT_TOKEN_AMOUNT_EXCEEDS_MAX_SUPPLY`.
    4.  **`recipient` Validation (if `recip_len > 0`):**
        * Resolved `recipient` string MUST NOT be empty. Error: `ERR_MINT_TOKEN_RECIPIENT_EMPTY`.
        * MUST be a valid account/identity identifier recognized by the host's ledger system. Error: `ERR_MINT_TOKEN_RECIPIENT_INVALID_FORMAT`.
        * The host MAY check if the recipient account exists or is capable of receiving tokens of `res_type`. Error: `ERR_MINT_TOKEN_RECIPIENT_UNKNOWN_OR_INCOMPATIBLE`.
    5.  **`recipient` Validation (if `recip_len == 0` - absent recipient):**
        * The host's schema for `res_type` MUST define a valid default recipient behavior (e.g., contract's own account, a designated treasury). If no default is defined and a recipient is mandatory for this `res_type`, `ERR_MINT_TOKEN_RECIPIENT_REQUIRED` MUST be raised.
    6.  **`data` Payload Validation (if `data_json_len > 0`):**
        * The resolved `data_json_string` MUST be successfully parsed as JSON. Error: `ERR_MINT_TOKEN_INVALID_DATA_JSON_PAYLOAD`.
        * The host's schema for `res_type` MAY define an expected structure/content for this `data` payload. The parsed JSON MUST be validated against this schema (Levels 1, 2, 3 as discussed for `host_set_property`). Error: `ERR_MINT_TOKEN_DATA_SCHEMA_VIOLATION`.
    7.  **Permissions/Capabilities:** The current contract execution context MUST have the permission to mint tokens of the specified `res_type` (and `amount`, to `recipient`, with `data`). Error: `ERR_MINT_TOKEN_PERMISSION_DENIED`.
    8.  **Ledger State:** The ledger for `res_type` MUST be initialized and active. Error: `ERR_MINT_TOKEN_LEDGER_NOT_INITIALIZED_OR_INACTIVE`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_MINT_TOKEN_NO_ACTIVE_CONTEXT` (if host policy requires for this action)
    * `ERR_MINT_TOKEN_RES_TYPE_EMPTY`
    * `ERR_MINT_TOKEN_RES_TYPE_UNKNOWN`
    * `ERR_MINT_TOKEN_RES_TYPE_INVALID_FORMAT`
    * `ERR_MINT_TOKEN_RES_TYPE_NOT_MINTABLE`
    * `ERR_MINT_TOKEN_AMOUNT_ZERO`
    * `ERR_MINT_TOKEN_AMOUNT_EXCEEDS_MAX_SUPPLY`
    * `ERR_MINT_TOKEN_AMOUNT_INVALID_DIVISIBILITY`
    * `ERR_MINT_TOKEN_RECIPIENT_REQUIRED` (if `recip_len == 0` and `res_type` needs explicit recipient)
    * `ERR_MINT_TOKEN_RECIPIENT_EMPTY` (if `recip_len > 0` but string is empty)
    * `ERR_MINT_TOKEN_RECIPIENT_INVALID_FORMAT`
    * `ERR_MINT_TOKEN_RECIPIENT_UNKNOWN_OR_INCOMPATIBLE`
    * `ERR_MINT_TOKEN_INVALID_DATA_JSON_PAYLOAD`
    * `ERR_MINT_TOKEN_DATA_SCHEMA_VIOLATION`
    * `ERR_MINT_TOKEN_PERMISSION_DENIED`
    * `ERR_MINT_TOKEN_LEDGER_NOT_INITIALIZED_OR_INACTIVE`
    * `ERR_MINT_TOKEN_LEDGER_OPERATION_FAILED` (generic failure during ledger update)
* **Interaction with Other Host Functions:**
    * `host_mint_token` directly changes the host's ledger state.
    * This change might be queryable by subsequent host functions invoked via `host_call_host`.
    * It could trigger host-emitted events (e.g., `tokens.minted`).

---
#### **Host Function 7: `if_condition_eval` (was `log_if_condition`)**

* **Note on Naming:** The original name in `emit.rs` is `log_if_condition`. For clarity in this ABI specification, we are describing the *required behavior*, which is primarily condition evaluation and flow control. The host implementation might choose a more descriptive internal name.
* **WASM Signature (from `emit.rs` type index 6):**
    `(param "condition_str_ptr" i32) (param "condition_str_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::If { condition: String }`.
* **Argument Interpretation:**
    * `condition_str_ptr`, `condition_str_len`: Host MUST read the UTF-8 string for the `condition` expression from WASM linear memory.
* **Host State Modification:**
    1.  The host MUST evaluate the resolved `condition` string using the "Condition Expression Language" (detailed in Section 4 of this ABI document) within the context of the current active data context (e.g., proposal, section).
    2.  Based on the boolean result of this evaluation (true or false), the host MUST push a new "conditional execution state" onto its internal conditional execution stack. This state object should record:
        * The result of the condition (e.g., `evaluated_to_true: bool`).
        * A flag indicating if an `host_else_handler` has been processed for this `If` block (e.g., `else_branch_taken: bool`, initialized to `false`).
    3.  This new state dictates whether subsequent opcodes (until a corresponding `host_else_handler` or `host_endif_handler`) are actively processed by the host or effectively skipped.
        * If `evaluated_to_true` is `true`, opcodes in the "then" branch are processed.
        * If `evaluated_to_true` is `false`, opcodes in the "then" branch are skipped.
* **Data Handling:**
    1.  Reads the `condition` string from WASM memory.
* **Validation Logic:**
    1.  **Active Data Context:** An active data context (proposal or section) MUST exist for the condition evaluation to occur, as expressions typically reference properties within this context. Error: `ERR_IF_NO_ACTIVE_CONTEXT`.
    2.  **Condition String Presence:** The resolved `condition` string MUST NOT be empty. Error: `ERR_IF_CONDITION_EMPTY`.
    3.  **Condition Expression Evaluation (refer to Section 4 for full language spec):**
        * The host MUST parse the `condition` string according to the defined "Condition Expression Language." Error: `ERR_IF_CONDITION_PARSE_ERROR`.
        * The host MUST evaluate the parsed expression. This involves:
            * Accessing properties from the current active data context (handling non-existent properties by raising an error, as per Section 4).
            * Applying comparison and logical operators.
            * Resolving literals.
        * Any failure during evaluation (e.g., referenced property not found, type mismatch during comparison if not handled by language coercions, division by zero if arithmetic allowed) MUST result in `ERR_IF_CONDITION_EVALUATION_ERROR`.
        * **Security:** The condition evaluator MUST be secure, sandboxed, and prevent arbitrary code execution or access to unauthorized host resources, strictly adhering to the defined expression language capabilities.
    4.  **Stack Depth:** The host MAY enforce a maximum nesting depth for conditional blocks. Error: `ERR_MAX_CONDITIONAL_DEPTH_REACHED`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_IF_NO_ACTIVE_CONTEXT`
    * `ERR_IF_CONDITION_EMPTY`
    * `ERR_IF_CONDITION_PARSE_ERROR`
    * `ERR_IF_CONDITION_EVALUATION_ERROR`
    * `ERR_MAX_CONDITIONAL_DEPTH_REACHED`
* **Interaction with Other Host Functions:**
    * Initiates a conditional block.
    * Its outcome (true/false evaluation) determines whether the host processes or skips opcodes that are translated into calls to other host functions (like `host_set_property`, `host_call_host`, etc.) until a `host_else_handler` or `host_endif_handler` is encountered.

---
#### **Host Function 8: `else_handler` (was `log_else`)**

* **Note on Naming:** Original name in `emit.rs` is `log_else`.
* **WASM Signature (from `emit.rs` type index 7):**
    `() -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::Else`.
* **Argument Interpretation:** None.
* **Host State Modification:**
    1.  The host MUST be in an active "conditional execution state" (i.e., the top of the conditional execution stack must correspond to a previously evaluated `host_if_condition_eval`).
    2.  The conditional state at the top of the stack MUST NOT have already processed an `host_else_handler` (i.e., its `else_branch_taken` flag must be `false`).
    3.  The host effectively inverts the execution permission for the remainder of the current conditional block:
        * If the original `If` condition was `true` (so the "then" branch was executed), subsequent opcodes (until `host_endif_handler`) are now skipped.
        * If the original `If` condition was `false` (so the "then" branch was skipped), subsequent opcodes (until `host_endif_handler`) are now processed.
    4.  The host MUST mark the current conditional state on its stack as having its `else_branch_taken` flag set to `true`.
* **Validation Logic:**
    1.  Conditional execution stack MUST NOT be empty.
    2.  The state at the top of the conditional execution stack MUST be an "if" state (initiated by `host_if_condition_eval`).
    3.  The `else_branch_taken` flag for the current conditional state MUST be `false`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ELSE_WITHOUT_IF` (if conditional stack is empty or top is not an 'if' state)
    * `ERR_ELSE_ALREADY_PROCESSED` (if `else_branch_taken` is already true for current 'if' state)
* **Interaction with Other Host Functions:**
    * Alters the execution flow for opcodes between it and `host_endif_handler`.
    * Must be within a block initiated by `host_if_condition_eval` and terminated by `host_endif_handler`.

---
#### **Host Function 9: `endif_handler` (was `log_endif`)**

* **Note on Naming:** Original name in `emit.rs` is `log_endif`.
* **WASM Signature (from `emit.rs` type index 8):**
    `() -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::EndIf`.
* **Argument Interpretation:** None.
* **Host State Modification:**
    1.  The host MUST be in an active "conditional execution state."
    2.  The host MUST pop the current conditional state from its conditional execution stack.
    3.  This restores the execution behavior of the parent scope (which might be unconditional or governed by an outer conditional block).
* **Validation Logic:**
    1.  Conditional execution stack MUST NOT be empty.
    2.  The state at the top of the conditional execution stack MUST be an "if" or "else" state (i.e., a state initiated by `host_if_condition_eval`).
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ENDIF_WITHOUT_IF` (if conditional stack is empty or top is not a valid conditional state)
* **Interaction with Other Host Functions:**
    * Terminates the current conditional execution block initiated by `host_if_condition_eval`.
    * Subsequent opcodes are processed according to the restored parent execution state.

---
#### **Host Function 10: `log_todo`**

* **WASM Signature (from `emit.rs` type index 10):**
    `(param "msg_ptr" i32) (param "msg_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::Todo(String msg)`.
* **Argument Interpretation:**
    * `msg_ptr`, `msg_len`: Host MUST read the UTF-8 string for the "todo" `msg` from WASM linear memory.
* **Host State Modification:**
    1.  This function's primary purpose is to log the provided message. It SHOULD NOT modify any core contract state or the host's primary context stack (e.g., proposal properties, section data).
    2.  The host MUST write the message to its designated logging system. The log entry SHOULD indicate that this message originates from a contract's "TODO" instruction.
    3.  The host MAY categorize these logs distinctly (e.g., at a "WARN" or "INFO" level, prefixed with "CONTRACT TODO:").
    4.  The host MAY increment internal metrics for "todo" logs if such diagnostics are tracked.
* **Data Handling:**
    1.  The host reads the `msg` string from WASM memory.
* **Validation Logic:**
    1.  **`msg` String Validation:**
        * The host MUST ensure the bytes read from `(ptr, len)` form a valid UTF-8 string. If not, `ERR_LOG_TODO_INVALID_UTF8` or a general `ERR_ABI_MEMORY_READ_FAILURE` (if the read itself indicates invalidity) SHOULD be raised.
        * The host MAY enforce a maximum length for the `msg` string to prevent log flooding or abuse (e.g., 1024 or 4096 characters). Error: `ERR_LOG_TODO_MSG_TOO_LONG`.
        * The host MAY sanitize the message for problematic control characters (beyond basic UTF-8 validity) before sending to certain log backends, though the primary responsibility for sending well-formed messages lies with the contract.
* **Return Value:** None (`-> ()`). Errors (like memory read failure or excessively long message if limit enforced) are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_LOG_TODO_MSG_TOO_LONG` (if a maximum length is enforced and violated)
    * `ERR_LOG_TODO_INVALID_UTF8` (if specific UTF-8 validation distinct from memory read is performed)
* **Interaction with Other Host Functions:**
    * `host_log_todo` is generally a terminal action for the information it carries; it does not directly alter the flow of other structural or state-modifying opcodes.
    * It can be called from within any context (global, proposal, section, conditional block, event handler) without altering that context's primary data.

---
#### **Host Function 11: `on_event`**

* **WASM Signature (from `emit.rs` type index 11):**
    `(param "event_ptr" i32) (param "event_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::OnEvent { event: String }`.
* **Argument Interpretation:**
    * `event_ptr`, `event_len`: Host MUST read the UTF-8 string for the `event` name from WASM linear memory.
* **Host State Modification:**
    1.  **Context Finalization & Transition:** If a previous top-level context (e.g., initiated by `host_create_proposal` or another `host_on_event`) was active on the host's main context stack, that context MUST be implicitly finalized (as per its specific finalization rules, e.g., a proposal context is committed). After finalization, it is popped or replaced.
    2.  **Event Handler Context Creation:** The host MUST then create a new "event handler definition context" specifically associated with the resolved `event` string.
    3.  This new context becomes the "current active context" (e.g., pushed onto the main context stack or set as the primary context).
    4.  The primary purpose of this context is to accumulate an ordered sequence of *action steps*. These action steps are defined by subsequent opcodes that translate to calls to host functions like `host_mint_token`, `host_anchor_data`, `host_call_host` (for action-oriented calls), `host_if_condition_eval` (for conditional actions), etc.
    5.  **Registration of Handler:** Upon the implicit finalization of this `host_on_event` block (which occurs when another top-level opcode like `host_create_proposal` or another `host_on_event` is encountered, or at the end of the WASM program execution), the collected sequence of action steps MUST be registered with the host's internal event dispatch system, keyed by the `event` string. This registration makes the handler available for execution when the host later triggers a matching event.
* **Data Handling:**
    1.  The host MUST read the `event` string from WASM memory.
    2.  The "event handler definition context" prepares to store an ordered list of action steps (representing the body of the handler).
* **Validation Logic:**
    1.  **State Validity:** `host_on_event` is typically a top-level construct. The host validates if it can transition from the previous context (if any) to defining an event handler.
    2.  **`event` String Validation:**
        * MUST NOT be empty. Error: `ERR_ON_EVENT_NAME_EMPTY`.
        * SHOULD have a host-enforced maximum length (e.g., 128-256 characters). Error: `ERR_ON_EVENT_NAME_TOO_LONG`.
        * SHOULD adhere to a defined format/namespace convention (e.g., `module.action`, `feature:event_name`, or simple `[a-zA-Z0-9_.-:]+`) to aid organization and prevent collisions. Error: `ERR_ON_EVENT_NAME_INVALID_FORMAT`.
        * The host MAY maintain a list of "reserved" or "system" event names that contracts cannot override or define handlers for in a typical manner. Error: `ERR_ON_EVENT_NAME_RESERVED`.
    3.  **Duplicate Handler Policy:**
        * The host MUST have a clear policy for handling an `Opcode::OnEvent` for an `event` string that already has a handler registered during the current contract execution/initialization phase.
        * **Recommended Policy:** Error on duplicate. `ERR_ON_EVENT_HANDLER_ALREADY_EXISTS`. (Alternatives like "replace with warning" are less safe for predictable behavior).
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_ON_EVENT_INVALID_STATE` (e.g., if trying to define an event handler in an invalid parent context, though typically these are top-level)
    * `ERR_ON_EVENT_NAME_EMPTY`
    * `ERR_ON_EVENT_NAME_TOO_LONG`
    * `ERR_ON_EVENT_NAME_INVALID_FORMAT`
    * `ERR_ON_EVENT_NAME_RESERVED`
    * `ERR_ON_EVENT_HANDLER_ALREADY_EXISTS` (if policy is to error on duplicates)
* **Interaction with Other Host Functions:**
    1.  `host_on_event` initiates a *definition block* for an event handler.
    2.  Subsequent host function calls that represent actions (e.g., `host_mint_token`, `host_anchor_data`, `host_perform_metered_action`, `host_transfer_token`, `host_call_host` for actions) and control flow (e.g., `host_if_condition_eval`, `host_else_handler`, `host_endif_handler`) are collected as part of this handler's definition.
    3.  The actual *execution* of these collected actions does not occur when `host_on_event` or the subsequent action-defining calls are processed. Execution happens *later*, when the host environment itself detects and triggers an event matching the registered `event` string (this triggering mechanism is outside the scope of this specific host function call but is a core part of the host's runtime).
    4.  The definition block is implicitly finalized by the next top-level opcode or the end of the WASM program. At this point, the fully defined handler is registered.

---
#### **Host Function 12: `log_debug_deprecated`**

* **WASM Signature (from `emit.rs` type index 12):**
    `(param "msg_ptr" i32) (param "msg_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Not directly tied to a primary, actively generated `Opcode` by the current `WasmGenerator`. Its name suggests it's for debugging or deprecated pathways. If called, it's expected to log the provided message.
* **Argument Interpretation:**
    * `msg_ptr`, `msg_len`: Host MUST read the UTF-8 string for the debug `msg` from WASM linear memory.
* **Host State Modification:**
    1.  This function's primary purpose is to log the provided debug message. It SHOULD NOT modify any core contract state or the host's primary context stack.
    2.  The host MUST write the message to its designated logging system. The log entry SHOULD indicate that this message originates from a contract's debug log call, possibly noting its deprecated nature.
    3.  The host MAY categorize these logs at a "DEBUG" level and MAY prefix them (e.g., "CONTRACT DEBUG (Deprecated Call):"). The host MAY also tag logs from this function with a `deprecated=true` flag in structured logging backends.
* **Data Handling:**
    1.  The host reads the `msg` string from WASM memory.
* **Validation Logic:**
    1.  **`msg` String Validation:**
        * The host MUST ensure the bytes read from `(ptr, len)` form a valid UTF-8 string. Error: `ERR_LOG_DEBUG_INVALID_UTF8` or `ERR_ABI_MEMORY_READ_FAILURE`.
        * The host MAY enforce a maximum length for the `msg` string to prevent log flooding (e.g., 1024 or 4096 characters). Error: `ERR_LOG_DEBUG_MSG_TOO_LONG`.
        * The host MAY sanitize the message for problematic control characters before logging.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_LOG_DEBUG_MSG_TOO_LONG` (if a maximum length is enforced and violated)
    * `ERR_LOG_DEBUG_INVALID_UTF8` (if specific UTF-8 validation distinct from memory read is performed)
* **Interaction with Other Host Functions:**
    * `host_log_debug_deprecated` is a terminal action for the information it carries.
    * It can be called from within any context without altering that context's primary data.
    * Given its "deprecated" nature, reliance on this function in new CCL contracts should be discouraged.
    * *For maintainers: Calls to `host_log_debug_deprecated` SHOULD be flagged by ABI-level linters or static analyzers as discouraged. Encourage use of structured logging via proposal contexts or event-based diagnostics instead.*

---
#### **Host Function 13: `range_check`**

* **WASM Signature (from `emit.rs` type index 13):**
    `(param "start_val" f64) (param "end_val" f64) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::RangeCheck { start: f64, end: f64 }`.
* **Note on Current Usage by `WasmGenerator`:**
    The `WasmGenerator` component (which translates the CCL DSL to Opcodes) primarily handles CCL range constructs (e.g., `key range X Y { /* rules */ }`) by emitting `Opcode::BeginSection` (with a kind like `"range_X_Y"`) and `Opcode::EndSection`, and processing the inner rules within that section. It does not typically emit `Opcode::RangeCheck` for these DSL-level range definitions.
    Therefore, this `host_range_check` function may be for legacy purposes, specific low-level checks not directly exposed by current CCL high-level syntax, or future use. The following specification describes its behavior when directly invoked.
* **Argument Interpretation:**
    * `start_val`: An `f64` representing the inclusive start of the range.
    * `end_val`: An `f64` representing the inclusive end of the range.
* **Host State Modification:**
    1.  This function, as named, primarily performs a validation of its own arguments.
    2.  Based purely on the provided arguments (`start_val`, `end_val`) and the lack of a value-to-check parameter in the opcode, this function currently has limited state-modifying utility beyond its argument validation.
    3.  **Future Extensions:** If future versions of `Opcode::RangeCheck` or associated WASM instruction sequences introduce mechanisms to associate this range with a contextual value (e.g., a preceding load or implicit property), this host function MAY be extended to perform full range validation against such a value and halt on violation, or to set a contextual range constraint. Such extensions would need to be clearly specified in future ABI versions.
* **Data Handling:**
    1.  Uses the `start_val` and `end_val` `f64` parameters.
* **Validation Logic:**
    1.  **Argument Sanity:** `start_val` MUST be less than or equal to `end_val`. If `start_val > end_val`, `ERR_RANGE_CHECK_INVALID_BOUNDS` MUST be raised.
    2.  **Contextual Value Check (Currently Underspecified by Opcode):**
        * As the opcode does not provide a value to check, no such check is performed by this host function based on the current ABI. Any such check would depend on future extensions or host-specific conventions outside this core definition.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_RANGE_CHECK_INVALID_BOUNDS` (if `start_val > end_val`)
    * `ERR_RANGE_CHECK_CONTEXTUAL_VALUE_UNAVAILABLE` (hypothetical, if an future extension for implicit value check was intended and the value was missing)
    * `ERR_RANGE_CHECK_VALUE_OUT_OF_BOUNDS` (hypothetical, if a future extension for implicit value check was intended and failed)
* **Interaction with Other Host Functions:**
    * In its current minimal interpretation (validating its own bounds only), it has very limited interaction with the state affected by other host functions.
    * Its "Note on Current Usage" and "Future Extensions" sections clarify that its role in complex range logic from CCL is superseded by the `host_begin_section` / `host_end_section` mechanism.

---
#### **Host Function 14: `use_resource`**

* **WASM Signature (from `emit.rs` type index 14):**
    `(param "resource_type_ptr" i32) (param "resource_type_len" i32) (param "amount" i64) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::UseResource { resource_type: icn_types::ResourceType /* enum */, amount: u64 }`. Note: The `resource_type` enum variant is converted to its string representation by `emit.rs` before this host function is called.
* **Argument Interpretation:**
    * `resource_type_ptr`, `resource_type_len`: Host MUST read the UTF-8 string for the `resource_type` from WASM linear memory. The host will need to parse this string to identify the specific resource (e.g., map it back to an internal `icn_types::ResourceType` enum or equivalent).
    * `amount`: Received directly as an `i64` (effectively `u64`) from the WASM stack, representing the quantity of the resource to be consumed.
* **Host State Modification (as per our refined spec for `Opcode::UseResource`):**
    1.  `host_use_resource` instructs the host to account for the consumption of a specified `amount` of a given `resource_type`.
    2.  The host MUST identify the current "execution context" (e.g., the currently running transaction or triggered event handler process). This execution context MUST have an associated resource budget.
    3.  Upon successful validation, the host MUST deduct the `amount` from the available balance of the specified (and parsed) `resource_type` within the current execution context's budget.
    4.  This operation MUST be atomic. If any part fails (e.g., validation, insufficient budget), the resource accounting state for this operation MUST NOT be changed (or must be rolled back).
* **Data Handling:**
    1.  Reads the `resource_type` string from WASM memory.
    2.  Uses the `amount` (`i64`) directly.
* **Validation Logic:**
    1.  **Active Execution Context & Budget:**
        * An active execution context with a defined resource budget MUST exist. Error: `ERR_USE_RESOURCE_NO_EXECUTION_CONTEXT`.
        * The host establishes this budget when the execution context is initiated (typically based on caller-provided limits or system defaults for that type of execution). The budget MUST contain an entry for the parsed `resource_type` or a general pool it can be mapped to. Error: `ERR_USE_RESOURCE_BUDGET_NOT_DEFINED` (if context exists but has no budget for this `resource_type`).
    2.  **`resource_type` String Validation & Parsing:**
        * The resolved `resource_type` string MUST NOT be empty. Error: `ERR_USE_RESOURCE_TYPE_EMPTY_STRING`.
        * The host MUST be able to parse/map this string to a known, meterable `icn_types::ResourceType` enum variant (or its internal equivalent). Error: `ERR_USE_RESOURCE_TYPE_UNKNOWN_STRING`.
        * The host's internal schema for this (parsed) `resource_type` MUST indicate that it is a metered resource. Error: `ERR_USE_RESOURCE_TYPE_NOT_METERED`.
    3.  **`amount` Validation:**
        * `amount` (as `u64`) MUST be greater than 0. Error: `ERR_USE_RESOURCE_AMOUNT_ZERO`.
        * The `amount` to be consumed MUST be less than or equal to the currently available balance of that `resource_type` in the execution context's budget. Error: `ERR_USE_RESOURCE_INSUFFICIENT_BUDGET`.
    4.  **Permissions/Capabilities:** While general execution implies some resource usage, the host MAY have finer-grained permissions for consuming specific, sensitive, or high-cost `resource_type`s, which would be checked here. Error: `ERR_USE_RESOURCE_PERMISSION_DENIED`.
    5.  **Quotas:** The host MAY enforce overall quotas on resource consumption (per contract, per user, per time window) that are checked in addition to the immediate execution context budget. Error: `ERR_USE_RESOURCE_QUOTA_EXCEEDED`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_USE_RESOURCE_NO_EXECUTION_CONTEXT`
    * `ERR_USE_RESOURCE_BUDGET_NOT_DEFINED`
    * `ERR_USE_RESOURCE_TYPE_EMPTY_STRING`
    * `ERR_USE_RESOURCE_TYPE_UNKNOWN_STRING` (if string cannot be mapped to a known `ResourceType` enum)
    * `ERR_USE_RESOURCE_TYPE_NOT_METERED`
    * `ERR_USE_RESOURCE_AMOUNT_ZERO`
    * `ERR_USE_RESOURCE_INSUFFICIENT_BUDGET`
    * `ERR_USE_RESOURCE_PERMISSION_DENIED`
    * `ERR_USE_RESOURCE_QUOTA_EXCEEDED`
    * `ERR_USE_RESOURCE_ACCOUNTING_FAILED` (generic internal error during debiting)
* **Interaction with Other Host Functions:**
    * `host_use_resource` directly depletes the current execution context's resource budget.
    * Its successful execution may be a prerequisite for other resource-intensive host functions (e.g., those called by `host_submit_job` or complex computations via `host_generic_call`).
    * Failure to secure resources via this function (e.g., due to insufficient budget) MUST halt further execution of the current logical operation/transaction to prevent unfunded resource use.

---
#### **Host Function 15: `transfer_token`**

* **WASM Signature (from `emit.rs` type index 15):**
    `(param "token_type_ptr" i32) (param "token_type_len" i32) (param "amount" i64) (param "sender_ptr" i32) (param "sender_len" i32) (param "recipient_ptr" i32) (param "recipient_len" i32) -> ()`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::TransferToken { token_type: String, amount: u64, sender: Option<String>, recipient: String }`.
* **Argument Interpretation:**
    * `token_type_ptr`, `token_type_len`: Host MUST read the UTF-8 string for the `token_type` from WASM linear memory.
    * `amount`: Received directly as an `i64` (effectively `u64`) from the WASM stack.
    * `sender_ptr`, `sender_len`: Host MUST read the UTF-8 string for the `sender`'s account identifier from WASM linear memory. If `sender_len` is 0, this indicates an absent/implicit sender (`None` in the Opcode), meaning the currently executing contract instance is the intended sender.
    * `recipient_ptr`, `recipient_len`: Host MUST read the UTF-8 string for the `recipient`'s account identifier from WASM linear memory.
* **Host State Modification (as per our refined spec for `Opcode::TransferToken`):**
    1.  This function instructs the host to attempt to move a specified `amount` of `token_type` from a `sender` account to a `recipient` account.
    2.  The host MUST identify the ledger associated with the resolved `token_type`.
    3.  **Sender Identification:**
        * If `sender_len == 0` (implicit sender), the sender account is the unique Contract Instance ID of the currently executing WASM module.
        * If `sender_len > 0`, the resolved `sender_str` MUST be validated (see Validation Logic). For this `host_transfer_token` function, it is mandated that this `sender_str` MUST be identical to the executing Contract Instance ID.
    4.  The identified sender account's balance for `token_type` MUST be debited by `amount`.
    5.  The resolved `recipient` account's balance for `token_type` MUST be credited by `amount`.
    6.  The entire operation (validation, debit from sender, credit to recipient) MUST be atomic. If any part fails, the transaction MUST be rolled back, leaving ledger balances unchanged.
* **Data Handling:**
    1.  Reads `token_type`, `sender` (if present), and `recipient` strings from WASM memory.
    2.  Uses `amount` (`i64`) directly.
* **Validation Logic:**
    1.  **Active Context:** The host MAY require an active context (e.g., `EventHandlerContext`, `ProposalContext`) for logging, or to associate the transfer with a broader operation, as per host policy.
    2.  **`token_type` Validation:**
        * Resolved `token_type` string MUST NOT be empty. Error: `ERR_TRANSFER_TOKEN_TYPE_EMPTY`.
        * MUST be a recognized token type with an active, transferable ledger in the host's resource/asset schema. Errors: `ERR_TRANSFER_TOKEN_TYPE_UNKNOWN`, `ERR_TRANSFER_TOKEN_TYPE_NOT_TRANSFERABLE`.
        * Host SHOULD validate format (max length, char set). Error: `ERR_TRANSFER_TOKEN_TYPE_INVALID_FORMAT`.
    3.  **`amount` Validation:**
        * `amount` (as `u64`) MUST be greater than 0. Error: `ERR_TRANSFER_TOKEN_AMOUNT_ZERO`.
        * MUST adhere to the divisibility rules for `token_type` (from its schema). Error: `ERR_TRANSFER_TOKEN_AMOUNT_INVALID_DIVISIBILITY`.
    4.  **`sender` Validation & Authorization:**
        * If `sender_len > 0`:
            * The resolved `sender_str` MUST NOT be empty. Error: `ERR_TRANSFER_TOKEN_SENDER_EMPTY`.
            * The `sender_str` MUST be a syntactically valid account identifier. Error: `ERR_TRANSFER_TOKEN_SENDER_INVALID_FORMAT`.
            * Crucially, `sender_str` MUST be identical to the unique Contract Instance ID of the currently executing WASM module. If not, `ERR_TRANSFER_TOKEN_SENDER_NOT_AUTHORIZED` MUST be raised. (This function does not handle delegated transfers from arbitrary senders; a separate mechanism like `host_transfer_from_approved` would be needed for that).
        * The identified sender account (the Contract Instance ID) MUST exist in the ledger for `token_type` and MUST have a sufficient balance (>= `amount`). Error: `ERR_TRANSFER_TOKEN_INSUFFICIENT_SENDER_BALANCE`. (If the contract instance account for a token type doesn't exist, it implies a balance of zero).
    5.  **`recipient` Validation:**
        * Resolved `recipient` string MUST NOT be empty. Error: `ERR_TRANSFER_TOKEN_RECIPIENT_EMPTY`.
        * MUST be a syntactically valid account identifier. Error: `ERR_TRANSFER_TOKEN_RECIPIENT_INVALID_FORMAT`.
        * Host policy dictates handling of transfers to recipient addresses that do not yet exist in the ledger for `token_type`. **Recommended:** Allow, implicitly creating the account balance entry.
        * The host MAY recognize specific "burn addresses" for `token_type` as valid recipients.
        * Self-transfers (`sender` effectively equals `recipient`) SHOULD be allowed (effectively a no-op on balance but may incur costs).
        * The host MAY check if the recipient account is capable of receiving the token type (e.g., if the recipient is a contract with specific receive hooks, though this is an advanced feature). Error (optional): `ERR_TRANSFER_TOKEN_RECIPIENT_INCOMPATIBLE`.
    6.  **Permissions/Capabilities:** General permission for the contract instance to initiate transfers of this `token_type` from its own account. Error: `ERR_TRANSFER_TOKEN_PERMISSION_DENIED`.
    7.  **Ledger State:** The ledger for `token_type` MUST be active and allow transfers. Error: `ERR_TRANSFER_TOKEN_LEDGER_INACTIVE`.
* **Return Value:** None (`-> ()`). Errors are signaled by halting execution.
* **Specific Error Codes (Host MUST halt execution):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_TRANSFER_TOKEN_CONTEXT_INVALID` (if host policy requires specific context)
    * `ERR_TRANSFER_TOKEN_TYPE_EMPTY`, `_UNKNOWN`, `_NOT_TRANSFERABLE`, `_INVALID_FORMAT`
    * `ERR_TRANSFER_TOKEN_AMOUNT_ZERO`, `_INVALID_DIVISIBILITY`
    * `ERR_TRANSFER_TOKEN_SENDER_EMPTY` (if `sender_len > 0` but string is empty)
    * `ERR_TRANSFER_TOKEN_SENDER_INVALID_FORMAT`
    * `ERR_TRANSFER_TOKEN_SENDER_NOT_AUTHORIZED` (if `sender` is specified but is not the current contract instance)
    * `ERR_TRANSFER_TOKEN_RECIPIENT_EMPTY`
    * `ERR_TRANSFER_TOKEN_RECIPIENT_INVALID_FORMAT`
    * `ERR_TRANSFER_TOKEN_RECIPIENT_UNKNOWN` (if host validates recipient existence and it's not found, and auto-creation is not policy)
    * `ERR_TRANSFER_TOKEN_RECIPIENT_INCOMPATIBLE` (optional)
    * `ERR_TRANSFER_TOKEN_INSUFFICIENT_SENDER_BALANCE`
    * `ERR_TRANSFER_TOKEN_PERMISSION_DENIED`
    * `ERR_TRANSFER_TOKEN_LEDGER_INACTIVE`
    * `ERR_TRANSFER_TOKEN_LEDGER_OPERATION_FAILED` (atomic rollback failed, or other ledger error)
* **Interaction with Other Host Functions:**
    * Directly alters ledger states by debiting the sender and crediting the recipient.
    * May trigger host-emitted events (e.g., `tokens.transferred`) which could, in turn, invoke `host_on_event` handlers in other contracts or the same contract.
    * The success/failure of a transfer can be a critical part of a larger workflow controlled by conditional logic (`host_if_condition_eval`) or a sequence of actions within an event handler or proposal.

---
#### **Host Function 16: `host_submit_mesh_job`**

* **WASM Signature (from `emit.rs` type index 16):**
    `(param "cbor_payload_ptr" i32) (param "cbor_payload_len" i32) (param "job_id_buffer_ptr" i32) (param "job_id_buffer_len" i32) -> i32`
* **Corresponding Opcode(s) in `emit.rs`:** Triggered by `Opcode::SubmitJob { manifest_cid: String, job_type: String, execution_model: String, required_resources_json: Option<String>, qos_profile_json: Option<String>, input_data_cid: Option<String>, output_instructions_cid: Option<String>, timeout_seconds: u64, max_retries: u32 }`.
    The `emit.rs` logic serializes these fields into a CBOR payload before calling this host function.
* **Argument Interpretation:**
    * `cbor_payload_ptr`, `cbor_payload_len`: Host MUST read the byte array from WASM linear memory. This byte array is expected to be a CBOR-serialized representation of all parameters originally in `Opcode::SubmitJob`. The host MUST deserialize this CBOR payload to reconstruct the job submission parameters (e.g., `manifest_cid`, `job_type`, etc.).
    * `job_id_buffer_ptr`, `job_id_buffer_len`: Host MUST interpret these as a pointer to a mutable buffer in WASM linear memory and its maximum capacity. If job submission is successful, the host will write the assigned `job_id` (as a UTF-8 string) into this buffer.
* **Host State Modification (as per our refined spec for `Opcode::SubmitJob`):**
    1.  This function instructs the host to validate, register, and attempt to schedule a new computational job.
    2.  Upon successful validation and resource commitment:
        * The host MUST assign a unique `job_id` (e.g., UUID string) to the job.
        * The host MUST write this `job_id` string into the WASM memory buffer specified by `job_id_buffer_ptr` and `job_id_buffer_len`. The actual number of bytes written (length of the `job_id` string) becomes part of the return value.
        * The host updates its internal job queue/scheduler with the new job and its parameters.
    3.  **Resource Commitment:** The host MUST ensure that resources for the job are accounted for and committed from the submitting Contract Instance's funds/allocations, as per the defined resource provisioning model (e.g., upfront hold of estimated cost, with final debit of actual cost upon job completion and refund of any difference). This MUST be atomic with job registration.
* **Data Handling:**
    1.  Reads the CBOR byte array from WASM memory and deserializes it to access the individual job submission parameters (e.g., `manifest_cid`, `job_type`, `required_resources_json` string, `qos_profile_json` string, etc.).
* **Validation Logic (applied to deserialized parameters from CBOR payload):**
    1.  **CBOR Deserialization:** The CBOR payload MUST be successfully deserialized into the expected structure containing all `Opcode::SubmitJob` parameters. Error: `ERR_SUBMIT_JOB_INVALID_CBOR_PAYLOAD`.
    2.  **Active Context:** An active context MAY be required by host policy for authorization or logging. Error: `ERR_SUBMIT_JOB_NO_ACTIVE_CONTEXT`.
    3.  **`manifest_cid` Validation:** Non-empty, valid CID format. Host MAY perform light, non-blocking pre-fetch/discoverability check (full resolution deferred to worker). Errors: `ERR_SUBMIT_JOB_MANIFEST_CID_EMPTY`, `_INVALID_FORMAT`, `_MANIFEST_POTENTIALLY_UNRESOLVABLE`.
    4.  **`job_type` Validation:** Non-empty, recognized by host, compatible with `execution_model`. Host needs a registry of supported job types. Errors: `ERR_SUBMIT_JOB_TYPE_EMPTY`, `_UNKNOWN`.
    5.  **`execution_model` Validation:** Non-empty, recognized by host, compatible with `job_type`. Errors: `ERR_SUBMIT_JOB_EXECUTION_MODEL_EMPTY`, `_UNKNOWN_OR_INCOMPATIBLE_WITH_JOB_TYPE`.
    6.  **`required_resources_json` Validation (if provided in CBOR):**
        * Must be a valid JSON string. Error: `ERR_SUBMIT_JOB_REQUIRED_RESOURCES_INVALID_JSON_STRING`.
        * The parsed JSON MUST conform to the host-defined schema for specifying resources (e.g., CPU, memory, storage, GPU). Error: `ERR_SUBMIT_JOB_REQUIRED_RESOURCES_SCHEMA_VIOLATION`.
    7.  **`qos_profile_json` Validation (if provided in CBOR):**
        * Must be a valid JSON string. Error: `ERR_SUBMIT_JOB_QOS_PROFILE_INVALID_JSON_STRING`.
        * The parsed JSON MUST conform to the host-defined schema for QoS parameters (e.g., priority, deadlines). Error: `ERR_SUBMIT_JOB_QOS_PROFILE_SCHEMA_VIOLATION`.
    8.  **`input_data_cid` / `output_instructions_cid` Validation (if provided in CBOR):** Must be structurally valid CIDs if present and non-empty. Errors: `ERR_SUBMIT_JOB_INPUT_CID_INVALID_FORMAT`, `ERR_SUBMIT_JOB_OUTPUT_INSTRUCTIONS_CID_INVALID_FORMAT`.
    9.  **`timeout_seconds` Validation:** Must be within a host-defined acceptable range. Error: `ERR_SUBMIT_JOB_TIMEOUT_OUT_OF_RANGE`.
    10. **`max_retries` Validation:** Must be within a host-defined acceptable range. Error: `ERR_SUBMIT_JOB_MAX_RETRIES_OUT_OF_RANGE`.
    11. **Resource Availability & Payment Validation:** The host MUST verify that sufficient resources/funds (based on `required_resources_json` or `job_type` cost model) are available from the submitting contract instance and can be committed/held. Error: `ERR_SUBMIT_JOB_INSUFFICIENT_RESOURCES_OR_FUNDS`.
    12. **Permissions/Capabilities:** The contract MUST have permission to submit jobs (possibly restricted by `job_type`, `execution_model`, or resource requests). Error: `ERR_SUBMIT_JOB_PERMISSION_DENIED`.
    13. **Quotas:** Host MAY enforce quotas. Error: `ERR_SUBMIT_JOB_QUOTA_EXCEEDED`.
    14. **`job_id_buffer` Capacity:** The provided `job_id_buffer_len` MUST be sufficient to hold the generated `job_id` string. Error: `ERR_SUBMIT_JOB_ID_BUFFER_TOO_SMALL`.
* **Return Value (`i32`):**
    * **Positive Value:** Success. The value is the length (number of bytes) of the `job_id` string written into the `job_id_buffer_ptr`. The `job_id` string itself SHOULD NOT include a null terminator as part of its returned length.
    * **Zero or Negative Values:** Indicate an error. These MUST correspond to specific, defined error codes (e.g., 0 for `ERR_SUBMIT_JOB_REGISTRATION_FAILED`, negative values for other specific errors).
* **Specific Error Codes (Returned as `i32` or signaled by halting for critical pre-condition failures):**
    * `ERR_ABI_MEMORY_READ_FAILURE`
    * `ERR_SUBMIT_JOB_INVALID_CBOR_PAYLOAD`
    * `ERR_SUBMIT_JOB_NO_ACTIVE_CONTEXT`
    * `ERR_SUBMIT_JOB_MANIFEST_CID_EMPTY`, `_INVALID_FORMAT`, `_MANIFEST_POTENTIALLY_UNRESOLVABLE`
    * `ERR_SUBMIT_JOB_TYPE_EMPTY`, `_UNKNOWN`
    * `ERR_SUBMIT_JOB_EXECUTION_MODEL_EMPTY`, `_UNKNOWN_OR_INCOMPATIBLE_WITH_JOB_TYPE`
    * `ERR_SUBMIT_JOB_REQUIRED_RESOURCES_INVALID_JSON_STRING`, `_SCHEMA_VIOLATION`
    * `ERR_SUBMIT_JOB_QOS_PROFILE_INVALID_JSON_STRING`, `_SCHEMA_VIOLATION`
    * `ERR_SUBMIT_JOB_INPUT_CID_INVALID_FORMAT`
    * `ERR_SUBMIT_JOB_OUTPUT_INSTRUCTIONS_CID_INVALID_FORMAT`
    * `ERR_SUBMIT_JOB_TIMEOUT_OUT_OF_RANGE`
    * `ERR_SUBMIT_JOB_MAX_RETRIES_OUT_OF_RANGE`
    * `ERR_SUBMIT_JOB_INSUFFICIENT_RESOURCES_OR_FUNDS`
    * `ERR_SUBMIT_JOB_PERMISSION_DENIED`
    * `ERR_SUBMIT_JOB_QUOTA_EXCEEDED`
    * `ERR_SUBMIT_JOB_ID_BUFFER_TOO_SMALL`
    * `ERR_SUBMIT_JOB_REGISTRATION_FAILED`
* **Interaction with Other Host Functions:**
    * `host_submit_mesh_job` is a major action initiating an asynchronous process.
    * Resource commitment logic is primarily handled by this function, potentially informed by prior `host_use_resource` calls for generic submission rights.
    * The `job_id` written to WASM memory can be used by subsequent calls to `host_generic_call` (e.g., to `query_job_status(job_id)`).
    * Job lifecycle events MAY trigger host-generated events handled by `host_on_event` mechanisms.

## 4. Condition Expression Language (CEL)

### 4.1. Overview

This section defines the Condition Expression Language (CEL), a mini-language used within ICN Contract Chain Language (CCL) to control conditional logic at runtime. These expressions are primarily utilized in the `condition` field of `Opcode::If` and are evaluated by the host function `host_if_condition_eval` (see Host Function 7).

CEL expressions operate by referencing properties and metadata from the current active context within the host environment. This can include properties of a proposal, fields of a section, or specific runtime values made available by the host. The language is designed to be simple, secure, and efficient for on-chain evaluation.

### 4.2. Syntax

CEL expressions are written using infix notation and are composed of literals, property references, operators, and optionally, built-in function calls.

#### 4.2.1. Literals
The following literal types are supported:
*   **Strings:** UTF-8 strings enclosed in double quotes (e.g., `"hello"`, `"v1.2.3"`). Special characters within strings (like `"` or `\`) must be appropriately escaped (e.g., `"\"hello\""` for `"hello"`).
*   **Numbers:** Integer and floating-point numbers (e.g., `0`, `42`, `3.14`, `-10.5`).
*   **Booleans:** `true` or `false`.
*   **Null:** `null`, representing an absent or undefined value.

#### 4.2.2. Property References
Values from the active host context are accessed using identifiers and dot notation:
*   **Identifiers:** Direct references to properties (e.g., `title`, `status`).
*   **Dot Notation:** Used for accessing nested properties or properties within specific scopes (e.g., `proposal.version`, `section.kind`, `props.amount.value`). See Section 4.4 for details on access scopes like `props`, `meta`, and `env`.

Property names are case-sensitive.

#### 4.2.3. Operators
The following operators are supported, listed in a typical order of precedence (though explicit grouping with parentheses is recommended for clarity):

| Category          | Operator(s) | Description                                   | Example                           |
|-------------------|-------------|-----------------------------------------------|-----------------------------------|
| Grouping          | `()`        | Controls order of evaluation                  | `(props.a && props.b) || props.c` |
| Unary Logical     | `!`         | Logical NOT                                   | `!props.is_closed`                |
| Comparison        | `<`, `<=`, `>`, `>=` | Less than, Less than or equal to, Greater than, Greater than or equal to | `props.score >= 75`            |
| Equality          | `==`, `!=`  | Equal to, Not equal to                        | `props.status == "open"`          |
| Logical AND       | `&&`        | Logical AND                                   | `props.isOpen && !props.hasVoted` |
| Logical OR        | `||`        | Logical OR                                    | `props.is_urgent || props.is_high_priority` |

#### 4.2.4. Function Calls (Optional Extension)
Future versions of CEL *may* include support for a limited set of built-in, side-effect-free functions. If implemented, they would follow a standard functional call syntax. Examples:
*   `length(props.title) > 10`
*   `contains(props.tags, "urgent")`
*   `startsWith(props.id, "proposal:")`

For version 1.0 of this ABI, direct function call syntax (e.g., `length(title) > 10`, `contains(props.tags, "urgent")`) within condition expressions is **not supported**. Hosts MUST reject expressions attempting to use such syntax, for example, by raising an `ERR_IF_CONDITION_PARSE_ERROR`. This syntax is reserved for potential future extensions of the CEL.

### 4.3. Evaluation Semantics

#### 4.3.1. Evaluation Order
*   Expressions are generally evaluated left-to-right, respecting operator precedence and parentheses.
*   **Short-circuit Evaluation:** Logical operators `&&` and `||` MUST use short-circuit evaluation:
    *   For `A && B`, if `A` evaluates to `false`, `B` is not evaluated, and the result is `false`.
    *   For `A || B`, if `A` evaluates to `true`, `B` is not evaluated, and the result is `true`.

#### 4.3.2. Null Behavior
*   The `null` literal represents an absent or undefined value.
*   **Strict Null Comparisons:** Any comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`) involving a `null` operand, where the other operand is not `null`, generally results in `false`, except for direct equality/inequality checks:
    *   `some_prop == null` evaluates to `true` if `some_prop` is indeed null/absent, `false` otherwise.
    *   `some_prop != null` evaluates to `true` if `some_prop` has a value, `false` if it's null/absent.
    *   Using `null` in arithmetic or other operations not explicitly defined for `null` will typically lead to a type error.

#### 4.3.3. Type Coercion
*   CEL is strongly typed with minimal implicit coercion to prevent unexpected behavior.
*   **No String-to-Number Coercion:** Strings representing numbers are not automatically coerced to numeric types for comparison or arithmetic.
    *   Example: `"5" == 5` MUST evaluate to `false`.
    *   Example: `"5" > 0` MAY result in a type error or be contextually defined (e.g. lexical string comparison if both operands are strings, type error if one is number and other is string).
*   Comparisons (`<`, `<=`, `>`, `>=`) are generally defined for operands of the same type (e.g., number with number, string with string lexically). Mixing types in these comparisons without explicit conversion functions (if available) typically results in a type error.
*   Boolean contexts (like in `if` statements or operands of `&&`, `||`, `!`) expect boolean values. Non-boolean values are typically not automatically coerced to booleans (e.g., `0` or `""` are not implicitly `false`).

#### 4.3.4. Evaluation Errors
During evaluation, if an error occurs, the `host_if_condition_eval` function MUST halt execution and signal an appropriate error. Common evaluation errors include:
*   Referencing a property that does not exist in the current context (see Section 4.4). Error: `ERR_IF_CONDITION_PROPERTY_NOT_FOUND`.
*   Attempting an operation with incompatible types (e.g., comparing a string with a number using `<`, or an arithmetic operation on a boolean). Error: `ERR_IF_CONDITION_TYPE_ERROR`.
*   Other runtime evaluation issues (e.g., division by zero if arithmetic operations were supported and used).

### 4.4. Property Access Model

All CEL expressions are evaluated within the scope of the "current active context" maintained by the host (e.g., a proposal context, section context). Properties are accessed using predefined namespaces or scopes:

*   **`props`**: Accesses properties explicitly set on the current active context object (e.g., a proposal's custom fields, a section's attributes).
    *   Example: `props.status`, `props.user_role`, `props.threshold_value`.
    *   If the current context is a section nested within a proposal, `props` refers to the section's properties. Access to parent proposal properties might require a different mechanism or be disallowed for simplicity.

*   **`meta`**: Accesses built-in metadata about the current active context or the execution environment. The exact available fields under `meta` are defined by the host schema.
    *   Example: `meta.context_kind` (e.g., `"proposal"`, `"section"`), `meta.context_id` (if applicable, e.g. proposal ID).

*   **`env`**: Accesses properties related to the broader runtime environment or transaction details. The exact available fields under `env` are defined by the host schema.
    *   Example: `env.current_timestamp` (e.g., block timestamp), `env.invoker_id` (e.g., the identity invoking the current transaction or contract).

**Path Resolution:**
*   Property paths are resolved using dot (`.`) notation (e.g., `props.author.did`).
*   If a segment in a path refers to a non-object or a non-existent intermediate property, attempting to access a sub-property from it will result in `ERR_IF_CONDITION_PROPERTY_NOT_FOUND`.

**Case Sensitivity:**
*   All property names and scope keywords (`props`, `meta`, `env`) ARE case-sensitive.

#### 4.4.1. Standard Context Properties

The `meta` and `env` scopes provide access to the following standard properties, if applicable to the current execution environment:

| Property Path         | Type   | Description                                                                                                | Availability         |
|-----------------------|--------|------------------------------------------------------------------------------------------------------------|----------------------|
| `meta.contract_id`    | String | The unique identifier (e.g., address or UUID) of the currently executing contract instance.                  | Always               |
| `meta.context_kind`   | String | The `kind` of the current active data context (e.g., `"proposal"`, `"section:role_attributes"`, `"event_handler"`). | When in data context |
| `meta.context_id`     | String | The unique ID of the current proposal context (if applicable, from `Opcode::CreateProposal`).                | In proposal context  |
| `meta.event_name`     | String | The name of the event being handled (if in an `host_on_event` initiated context).                          | In event handler ctx |
| `env.timestamp_ns`    | Number | The current host-provided timestamp as nanoseconds since the Unix epoch.                                     | Always               |
| `env.block_height`    | Number | The current block height or equivalent transaction ordering mechanism from the host ledger (if applicable).  | If host is ledger-based |
| `env.transaction_id`  | String | A unique identifier for the current transaction or execution trace being processed by the host.            | Always               |
| `env.actor_did`       | String | The Decentralized Identifier (DID) of the agent (user, contract, or system entity) that initiated the current execution trace or transaction. | If applicable        |

The host schema (to be detailed further in Section 5) may define additional or environment-specific `meta` and `env` properties.

### 4.5. Examples

1.  Check if a proposal's status is "open" and the current invoker is not the author:
    ```cel
    props.status == "open" && env.invoker_id != props.author_id
    ```

2.  Check if the context is a "proposal" and its score is at least 80:
    ```cel
    meta.context_kind == "proposal" && props.score >= 80
    ```

3.  (Illustrative, assuming `length` and `contains` functions are supported): Check if a list of tags has more than two items and includes "emergency":
    ```cel
    length(props.tags) > 2 && contains(props.tags, "emergency")
    ```

4.  Check for the presence of an optional title:
    ```cel
    props.title != null
    ```

### 4.6. Host Implementation Requirements

The host implementation of the CEL evaluator MUST adhere to the following:

1.  **Security:** The evaluator MUST be secure and sandboxed to prevent any form of arbitrary code execution or unauthorized access to host resources beyond the explicitly defined property access model.
2.  **Determinism:** For a given context and expression, evaluation MUST be deterministic.
3.  **Error Handling:**
    *   If the condition string fails to parse according to CEL syntax, the host MUST signal `ERR_IF_CONDITION_PARSE_ERROR`.
    *   If an error occurs during evaluation (e.g., property not found, type mismatch), the host MUST signal the appropriate error, such as `ERR_IF_CONDITION_EVALUATION_ERROR`, `ERR_IF_CONDITION_PROPERTY_NOT_FOUND`, or `ERR_IF_CONDITION_TYPE_ERROR`.
    *   All such errors MUST halt further execution of the CCL.
4.  **Resource Limits:** The host MAY impose limits on expression complexity, length, or evaluation steps to prevent abuse or excessive resource consumption. Exceeding these limits should result in an appropriate error (e.g., `ERR_IF_CONDITION_COMPLEXITY_EXCEEDED`).

--- 

## 5. Schema Conventions

### 5.0. Overview and Schema Evolution Policy

The ICN Host ABI relies heavily on structured schemas to validate contract-provided data across a wide range of host functions. These schemas define the expected format, type, and semantic constraints for:

- Proposal metadata and nested sections,
- Properties and context structure,
- Token and resource type identifiers,
- Job submission payloads and execution parameters,
- Generic host function calls,
- And host-provided context fields such as `meta.*` and `env.*`.

Unless otherwise specified, schemas defined in this section are **normative** and **MUST be enforced** by host implementations at runtime. The integrity of ICN contracts depends on consistent enforcement of these validation rules across all federated nodes.

#### 5.0.1. Schema Evolution Policy

To ensure compatibility and safety across upgrades, the following policies govern schema evolution:

- **Backward-compatible, additive changes are permitted.**
  - Examples include: introducing new optional fields, expanding allowed enum values, adding new section kinds.
- **Breaking changes to existing required fields, constraints, or types are not allowed** in the same ABI version.
- **Schema versions MAY be introduced per host-defined registry** for experimental or host-specific schema variants, but MUST be clearly namespaced or flagged (e.g., `kind: "myhost::role_attributes_v2"`).

#### 5.0.2. Host-Specific Schema Extensions

Hosts MAY define additional schemas beyond those listed here for:

- Custom `host_generic_call` functions,
- Additional section kinds or event names,
- New resource or token types.

However, such extensions MUST:

- Not conflict with any standard schema name or kind,
- Be clearly prefixed or namespaced to avoid ambiguity,
- Not alter the behavior of standard host functions defined in Section 3 unless otherwise permitted.

---

### 5.1. Introduction to Host Schemas

This section defines the **structural expectations** and **validation rules** that hosts use to interpret contract-supplied data within the ICN ABI. These schemas govern not only data formats but also runtime behavior such as property validation, proposal nesting, token issuance, and job scheduling.

#### 5.1.1. Purpose and Role in ABI Validation

Each host function described in Section 3 relies on schema-driven logic to determine whether the incoming data:

- Is syntactically well-formed,
- Conforms to expected field types and constraints,
- Is semantically valid for the current context (e.g., a proposal's title being a non-empty string),
- Obeys contextual relationships (e.g., only certain section kinds can appear within a specific proposal schema).

By defining these schemas in a shared canonical document, hosts can guarantee uniform behavior across federations, and contract authors can reliably target a single ABI standard.

#### 5.1.2. General Principles

All schemas in this section are defined using the following conventions:

- **Types**: Primitive types follow JSON semantics: `string`, `number`, `boolean`, `object`, `array`, and `null`.
- **Optionality**:
  - Fields marked as **required** MUST be present and valid.
  - Fields not marked as required are **optional**; if present, they MUST still pass validation.
- **Length and format constraints**:
  - String fields may have length limits or character set restrictions.
  - Numeric fields may specify ranges (e.g., `0 <= term_limit <= 100`).
- **Nested Objects**: Objects may contain required or optional subfields with their own validation rules.
- **Arrays**: May specify item type, uniqueness, minimum/maximum length.

#### 5.1.3. Schema Language

For readability and consistency with the rest of this document, schemas are primarily presented **narratively** in Markdown tables.

Each schema definition will include:

- A **field table** listing:
  - Field name
  - Type
  - Whether the field is required
  - Description
  - Constraints (e.g., regex, min/max)
- An optional **example JSON snippet** showing a valid payload.
- Commentary on context-specific constraints (e.g., which host function uses this schema).

While formal JSON Schema could be used in the future for machine validation or code generation, this ABI document prioritizes clarity and conciseness for human implementers.

--- 

### 5.2. Global Naming and Identifier Conventions

Many fields within CCL contracts and host-invoked operations rely on structured identifiers — including `section.kind`, `event_name`, `job_type`, `execution_model`, and resource or token type strings. To ensure host interoperability, schema validation, and future-proof naming, this section defines the canonical conventions for these identifiers.

#### 5.2.1. Identifier Grammar Overview

All identifiers must be valid UTF-8 strings and SHOULD adhere to the character and structure constraints below. Hosts MAY enforce stricter limits for specific contexts, but these global rules define the baseline expectations for all identifiers used in standard ICN contracts.

| Identifier Type           | Allowed Characters / Pattern                 | Format Rule                                     | Example(s)                                  |
|--------------------------|----------------------------------------------|--------------------------------------------------|----------------------------------------------|
| `section.kind`           | `[a-z0-9_]+`                                 | Lowercase `snake_case`                          | `role`, `role_attributes`, `membership`      |
| `event_name`             | `[a-zA-Z0-9_.:-]+`                           | Dot-separated namespaces; optional colons       | `tokens.transferred`, `proposal:submitted`   |
| `job_type`               | `[a-z0-9/_]+`                                | Slash-separated identifiers                     | `wasm/v1`, `python/fastapi`                  |
| `execution_model`        | `[a-z0-9/_]+`                                | Slash-separated identifiers                     | `wasmtime/ephemeral`, `native/container`     |
| `resource_type`          | `[a-z0-9:_]+`                                | Colon-separated namespace identifiers           | `icn::nfr`, `cooperative:mana`, `token:usd`  |
| `token_type`             | `[a-z0-9:_]+`                                | Same as `resource_type`                         | `coop:credit`, `token:nfr`, `token:usdv`     |

---

#### 5.2.2. Identifier Rules and Rationale

**section.kind**  
Used in `host_begin_section`. Defines the semantic "kind" of a section. Reserved kinds like `role`, `role_attributes`, `budget`, `proposal_meta`, etc., are defined in Section 5.3. Must be lowercase, short, and unambiguous.

**event_name**  
Used in `Opcode::OnEvent` and `host_on_event`. Follows a dot-separated hierarchical structure (similar to logging systems). Optional colons may indicate host-reserved or system-defined events. Contract-defined names SHOULD use reverse-DNS or cooperative prefixes to avoid collisions (e.g., `mycoop.myapp.my_event`).

**job_type**  
Used in `Opcode::SubmitJob`. Describes the type of workload. Host implementations SHOULD register supported job types along with schemas in Section 5.5. Must be lowercase and versioned using slashes (`/`), not colons.

**execution_model**  
Also used in `Opcode::SubmitJob`. Describes how the job is executed (e.g., interpreter, container). Typically pairs with `job_type` to determine environment.

**resource_type** and **token_type**  
Used across `host_use_resource`, `host_transfer_token`, and `host_mint_token`. Must use consistent namespace structure (`host::`, `coop:`, `token:`). Names MUST NOT conflict with system-defined tokens unless explicitly whitelisted by the federation or host configuration. Hosts MAY reject unknown `resource_type`/`token_type` values unless they are pre-registered.

---

#### 5.2.3. Reserved and Recommended Prefixes

To ensure forward compatibility and avoid collision between standard and application-specific identifiers, the following prefixes are reserved or recommended:

| Prefix        | Purpose                            | Notes                                            |
|---------------|------------------------------------|--------------------------------------------------|
| `icn::`       | Core system-wide types             | Reserved for ICN-provided types (e.g., `nfr`)   |
| `token:`      | Tokenized economic instruments     | Standard for token identifiers                  |
| `coop:`       | Cooperative-defined types          | Each cooperative SHOULD namespace its tokens    |
| `host:`       | Host environment/system resources  | Used by hosts for internal metrics or throttles |
| `proposal:`   | Event types related to proposals   | Reserved for system event names                 |
| `wasm/`       | Job types or execution models      | Indicates Wasm-based workloads                  |
| `native/`     | Non-Wasm workloads                 | Used for native, containerized, or bridged jobs |

---

#### 5.2.4. Identifier Length and Encoding Constraints

- All identifiers MUST be valid UTF-8 strings.
- Length SHOULD NOT exceed 128 characters.
- Identifiers MUST NOT contain unprintable control characters.
- Hosts MAY canonicalize identifiers (e.g., trim whitespace, lowercase certain parts) before processing, but SHOULD raise errors on ambiguous or malformed input.

---

#### 5.2.5. Host Validation and Extensibility

- Hosts MUST validate all identifier strings in contexts where they are schema-bound.
- Unknown identifiers MAY be rejected or MAY trigger fallback behavior based on host policy (e.g., default resource denial).
- Schema-defined identifiers SHOULD be documented in Sections 5.3 through 5.6.

---

### 5.3. Structural Schemas (for Proposals, Sections, and their Properties)

#### 5.3.0. Introduction to Structural Schemas
Purpose: Defines the structure of proposals, sections, and event handlers.
Focus on `kind` attribute, properties, parent/child relationships.
**Note:** Direct action Opcodes (like `Opcode::MintToken`, `Opcode::SubmitJob`, `Opcode::TransferToken`) are not `section.kind`s and are validated based on their own parameters as defined in their respective Host Function specifications (Section 3), even if they appear within contexts like event handlers. Section 5.3 primarily deals with the schema of explicit sections created via `host_begin_section`.

---
#### 5.3.1. Top-Level Module Schemas: Proposal Context
*(Details to be added for the overall Proposal context, including its standard properties like id, title, version, and permitted top-level child section kinds.)*

---
#### 5.3.2. Common Section `kind` Schemas
This subsection details schemas for common, general-purpose section kinds that can be broadly utilized across various contexts within a proposal or other top-level structures. These sections typically provide descriptive information, link to external resources, or group related data anchors. Examples include `metadata`, `terms_and_conditions`, and `data_anchors_list`.

---
##### 5.3.2.1. `section.kind: metadata`

*   **Description:**
    Provides a flexible mechanism for attaching arbitrary, structured key-value metadata to any context (e.g., a proposal, another section, a role definition, a budget). This allows for rich, non-operational descriptive data, links, annotations, or other contextual information not covered by more specific section kinds.

*   **Expected Parent `kind`(s) or Context:**
    *   `proposal`
    *   Any other section `kind` (e.g., `role`, `budget_definition`, `permissions_policy`, even another `metadata` section for nesting)
    *   Essentially, any context where attaching general key-value metadata is relevant.

*   **Permitted Child `section.kind`(s):**
    *   `metadata` (0 or more - allowing for nested metadata structures if complex information needs to be organized hierarchically)
    *   Other specific section kinds MAY be permitted by a host's schema if the `metadata` section serves as a grouping context for them, but typically `metadata` sections are leaf nodes or only contain further `metadata` children.

*   **Schema Table for `props`:**
    The `props` object of a `metadata` section is itself the collection of metadata. It consists of arbitrary key-value pairs.

    | Category         | Constraint                                                                                                                               |
    |------------------|------------------------------------------------------------------------------------------------------------------------------------------|
    | Keys             | MUST be valid UTF-8 strings. SHOULD follow a consistent naming convention (e.g., `snake_case` or `camelCase`) and avoid characters that might conflict with CEL pathing (e.g. `.`, `[`, `]`) if these properties might be referenced. Max length of 128 characters is recommended. |
    | Values           | CAN be any valid JSON type: `string`, `number`, `boolean`, `array`, or `object` (allowing for nested structures), or `null`.                 |
    | Required Keys    | None are mandated by the `metadata` schema itself. The contract author determines the necessary keys for their use case.                     |
    | Host Interpretation| Generally, the host treats these properties as opaque data provided by the contract. However, specific keys MAY be recognized by host extensions or higher-level application schemas if prefixed (e.g., `app::my_data_id`). |
    | Size/Depth Limits| Hosts MAY impose limits on the total size of the `props` object, the number of keys, or the nesting depth of objects/arrays within values to prevent abuse. |

*   **Notes:**
    *   The `metadata` section is designed for flexibility. Its primary role is to store descriptive or referential information that doesn't fit into strictly typed sections.
    *   While keys are arbitrary, contracts SHOULD use clear, descriptive, and consistently cased keys.
    *   For complex, highly structured metadata that might be queried or validated frequently, defining a custom section `kind` with a specific schema might be more appropriate than using a generic `metadata` section.

*   **Example JSON Snippet:**

    ```json
    {
      "kind": "metadata",
      "title": "Additional Project Details",
      "props": {
        "project_code": "ICN-ALPHA-007",
        "external_tracking_url": "https://tracker.example.com/issue/ICN-789",
        "responsible_team": "Core Development",
        "tags": ["governance", "milestone-2", "api-design"],
        "review_status": {
          "status_code": "pending_community_review",
          "last_updated": "2024-07-15T10:00:00Z",
          "reviewer_pool": ["role:auditor", "did:coop:membergroupX"]
        },
        "related_documents_cids": [
          "bafybeiaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "bafybeibbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ]
      }
    }
    ```

---
#### 5.3.3. Role-Based Access Control (RBAC) and Governance Schemas
This subsection details schemas for defining roles, their attributes, associated permissions, and membership rules, forming the foundation for governance and access control within ICN contracts.

##### 5.3.3.1. `section.kind: role_definition_group`
*(Details to be added for this container section, which is used to group multiple `role` definitions, for instance, in a global policy document or a dedicated RBAC configuration proposal.)*

---
##### 5.3.3.2. `section.kind: role`

*   **Description:** Defines a specific role within a governance structure or proposal. It acts as a primary container for attributes, permissions, and membership rules related to that role.
*   **Expected Parent `kind`(s) or Context:**
    *   `proposal` (when roles are defined directly as top-level sections within a proposal)
    *   `role_definition_group` (when roles are organized within a dedicated grouping section)
*   **Permitted Child `kind`(s):**
    *   `role_attributes` (Min: 1, Max: 1 - Every `role` section MUST contain exactly one `role_attributes` child section to define its characteristics.)
    *   `permissions_policy` (Min: 0, Max: 1 - A `role` section MAY contain zero or one `permissions_policy` child section to define specific permissions granted to this role.)
    *   `membership_rules` (Min: 0, Max: 1 - A `role` section MAY contain zero or one `membership_rules` child section to define how actors become members of this role.)
*   **Schema Table for `props` (Properties of the `role` section itself):**

    | Property Key | Type   | Required | Constraints / Description                                                                                                                                                             |
    |--------------|--------|----------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
    | `role_name`  | String | Yes      | The unique name for the role within its defining context (e.g., unique among all roles in a `role_definition_group` or unique among top-level roles in a proposal). Adheres to identifier conventions in 5.2 (e.g., `[a-zA-Z0-9_-]+`), max length 64 characters. Example: "chairperson", "voter_level_1", "auditor". |
    | `description`| String | No       | A brief, human-readable description of the role's purpose, responsibilities, and scope. Max length 512 characters.                                                                      |

*   **Example JSON Snippet (Illustrating a `role` section with its children):**

    ```json
    {
      "kind": "role",
      "title": "Auditor Role Definition", 
      "props": {
        "role_name": "auditor",
        "description": "Responsible for reviewing and verifying operational compliance and financial records."
      },
      "sections": [ 
        {
          "kind": "role_attributes",
          "title": "Auditor Attributes",
          "props": { 
            "term_length_days": 730,
            "seats": 2,
            "requirements_summary": "Minimum 5 years relevant experience, specific certifications.",
            "election_process_ref": "doc:election_policy_v2#auditors"
          }
          
        },
        {
          "kind": "permissions_policy",
          "title": "Auditor Permissions",
          "props": { 
            "policy_id": "auditor_permissions_v1.2",
            "rules_cid": "bafybeiccfsaq4a67h3...", 
            "default_effect": "deny"
          }
        },
        {
          "kind": "membership_rules",
          "title": "Auditor Membership Rules",
          "props": {
            "application_process_url": "https://example.com/apply/auditor",
            "min_stake_required": "1000icn::nfr"
          }
        }
      ]
    }
    ```

---
##### 5.3.3.3. `section.kind: role_attributes`

* **Description:**  
  Defines the core operational parameters, eligibility criteria, and lifecycle rules associated with a `role`. This section contains structured metadata governing how the role functions, including term lengths, seat count, and required qualifications. It must be present exactly once within every `role` section.

* **Expected Parent `kind`:**
  * `role`

* **Permitted Child `section.kind`s:**
  * _None._ This section is considered terminal and is composed solely of structured `props`. If a host permits specialized nested definitions (e.g., `eligibility_policy`), such extensions MUST be namespaced and documented via the schema evolution process (Section 5.0).

* **Schema Table for `props` (Properties of `role_attributes`):**

| Property Key           | Type     | Required | Description                                                                                          |
|------------------------|----------|----------|------------------------------------------------------------------------------------------------------|
| `term_length_days`     | Number   | Yes      | Total length (in days) of a single elected or appointed term for this role. Must be positive.        |
| `term_limit`           | Number   | No       | Maximum number of terms an individual may serve in this role. If absent, no limit is enforced.       |
| `seats`                | Number   | Yes      | Number of concurrent holders (seats) allowed for this role. Must be ≥ 1.                             |
| `requirements_summary`| String   | No       | A short, human-readable description of the eligibility criteria. Max 512 characters.                 |
| `election_process_ref`| String   | No       | A URL or CID reference to a document describing the election or appointment process for this role.   |
| `requirements`         | Object   | No       | A structured, machine-readable object expressing eligibility conditions. See nested schema below.    |

* **Nested `requirements` object schema:**

| Field Key           | Type     | Required | Description                                                                               |
|---------------------|----------|----------|-------------------------------------------------------------------------------------------|
| `experience_years`  | Number   | No       | Minimum number of years of relevant experience required. Must be ≥ 0 if present.          |
| `certifications`    | Array of String | No | Required certifications (e.g., ["CPA", "ISO-Auditor"]).                                  |
| `must_be_member`    | Boolean  | No       | Whether the individual must already be a registered member of the cooperative.            |
| `background_check_required` | Boolean | No | Whether a formal background check is a prerequisite for eligibility.                     |

* **Example JSON Snippet:**

```json
{
  "kind": "role_attributes",
  "title": "Auditor Role Attributes",
  "props": {
    "term_length_days": 730,
    "term_limit": 2,
    "seats": 2,
    "requirements_summary": "Must have a financial auditing background and clean disciplinary record.",
    "election_process_ref": "doc:election_policy_v2#auditors",
    "requirements": {
      "experience_years": 5,
      "certifications": ["CPA", "ISO-Auditor"],
      "must_be_member": true,
      "background_check_required": true
    }
  }
}
```

---

*Notes:*

* This section is **required** within a `role` definition. Hosts MUST enforce its presence and validate all required fields as per the schema.
* Optional fields may be omitted unless mandated by host governance rules.
* Hosts MAY choose to extend the `requirements` object schema, but such extensions MUST use namespaced keys (e.g., `coop:training_completed`) and follow the schema evolution guidelines.

---

##### 5.3.3.4. `section.kind: permissions_policy`
*   **Description:**  
    Defines the permission policy associated with a `role`, governing what actions members of that role are allowed (or denied) to perform within the execution context of a contract or federation. This policy is typically represented by a referenced ruleset (e.g. CID-anchored JSON logic), allowing host engines to evaluate permissions during runtime enforcement.

*   **Expected Parent `kind`(s) or Context:**
    *   `role` (MUST be the immediate parent)

*   **Permitted Child `section.kind`s:**
    *   _None._ This is a terminal section focused entirely on structured `props`. If needed, complex policies should be encoded in the referenced `rules_cid`.

*   **Schema Table for `props`:**

| Property Key   | Type   | Required | Constraints / Description                                                                                           |
|----------------|--------|----------|---------------------------------------------------------------------------------------------------------------------|
| `policy_id`    | String | Yes      | A unique identifier for the permission policy (e.g., "auditor_policy_v1", "admin_default"). Max length: 64 chars. |
| `rules_cid`    | String | Yes      | CID pointing to the structured permission ruleset (e.g., JSON logic or rules DSL). MUST be a valid CID.             |
| `default_effect`| String | Yes      | "allow" or "deny" — specifies the default behavior when no rule matches. Case-insensitive, validated against enum. |
| `description`  | String | No       | Optional human-readable explanation of the policy. Max length: 512 characters.                                     |
| `schema_version`| String | No       | Optional semantic version string identifying the expected structure of the ruleset at `rules_cid`. E.g., "1.0.0"    |

*   **Notes:**
    *   The referenced CID (`rules_cid`) must resolve to a valid, host-parsable ruleset document. This may be a JSON object conforming to a permission rules DSL (e.g., attribute-based access control).
    *   `default_effect` is used as a fallback decision during policy evaluation and must be either "allow" or "deny". Hosts may normalize case or enforce strict casing depending on configuration.
    *   The `policy_id` is local to the proposal or section scope and does not need to be globally unique, but SHOULD be unique among other permission policies within the same `role_definition_group` or contract.
    *   Hosts MAY cache or index permission policies using the `policy_id` and `rules_cid` for fast runtime evaluation.
    *   Policies are enforced at runtime for any action that triggers an access decision (e.g., job submission, token transfer, configuration change) depending on host integration.

*   **Example JSON Snippet:**

```json
{
  "kind": "permissions_policy",
  "title": "Auditor Permissions",
  "props": {
    "policy_id": "auditor_policy_v1",
    "rules_cid": "bafybeih2py3brfd7h6ylokq6elwrciym6f5mcqtwkwh3woebq7xtnbxzga",
    "default_effect": "deny",
    "description": "Auditors may access and verify budget sections, but may not modify them.",
    "schema_version": "1.0.0"
  }
}
```
---

---
##### 5.3.3.5. `section.kind: membership_rules`

*   **Description:** Defines structured rules governing how actors may become members of a role. This includes minimum eligibility, application process, and optionally, a CID-linked ruleset. Every `role` MAY contain one `membership_rules` section.

*   **Expected Parent `kind`:**
    *   `role`

*   **Permitted Child `section.kind`s:**
    *   _None._ This section is terminal and composed entirely of `props`.

---

**Schema Table for `props`:**

| Property Key             | Type   | Required | Description                                                                                         |
|--------------------------|--------|----------|-----------------------------------------------------------------------------------------------------|
| `application_process_url`| String | Yes      | URL describing the process for applying to this role. Max 512 characters.                          |
| `min_stake_required`     | String | Yes      | Minimum stake required for membership (e.g., `"1000icn::nfr"`). Format must follow token type rules. |
| `rules_cid`              | String | No       | CID pointing to a structured JSON ruleset (e.g., ABAC conditions, eligibility filters).            |
| `description`            | String | No       | Optional human-readable summary of how membership is determined. Max 512 characters.               |

---

**Notes:**
- `application_process_url` and `min_stake_required` are REQUIRED.
- `rules_cid` is OPTIONAL. If present, the host MUST validate the content format and schema at the referenced CID.
- If both inline fields and a `rules_cid` are present, the host MAY enforce both, with `rules_cid` taking precedence in case of conflict.

---

**Example JSON Snippet:**

```json
{
  "kind": "membership_rules",
  "title": "Auditor Membership Rules",
  "props": {
    "application_process_url": "https://example.com/apply/auditor",
    "min_stake_required": "1000icn::nfr",
    "description": "Applicants must stake at least 1000 ICN-NFR tokens and complete an application process."
  }
}
```
---

---

#### 5.3.4. Financial and Resource Management Schemas
This subsection details schemas for defining and managing financial budgets, resource allocations, and other economic activities within ICN contracts.

---
##### 5.3.4.1. `section.kind: budget_definition`

* **Description:**  
  Defines a resource or token-based budget within a proposal. Budgets establish limits or allocations of resources (such as token funds or compute `mana`) across categories, time periods, or recipient groups. This section can be used to predefine spending authority, operational funding, role compensation, or execution reserves.

* **Expected Parent `kind`:**
  * `proposal`
  * `allocation_plan` (optional intermediate grouping section)

* **Permitted Child `section.kind`s:**
  * `allocation_request` (0 or more — each sub-request allocates part of this budget to a recipient, use-case, or phase)

* **Schema Table for `props`:**

| Property Key           | Type    | Required | Description                                                                                          |
|------------------------|---------|----------|------------------------------------------------------------------------------------------------------|
| `budget_name`          | String  | Yes      | Short human-readable label for the budget. Must follow `[a-zA-Z0-9_-]{1,64}`.                        |
| `token_type`           | String  | Yes      | The token or resource being budgeted. Must follow `resource_type` or `token_type` naming conventions from Section 5.2. Examples: `icn::nfr`, `coop:mana`. |
| `total_amount`         | Number  | Yes      | The total amount of the token/resource authorized in this budget. Must be ≥ 0.                       |
| `budget_period_days`   | Number  | No       | Optional duration of the budget in days (e.g., `30`, `365`). If omitted, budget is one-time or unlimited duration. |
| `recurring`            | Boolean | No       | If `true`, the budget renews automatically at each `budget_period_days` interval. Defaults to `false`. |
| `vesting_schedule_cid` | String  | No       | Optional CID pointing to a document that defines a more complex schedule for vesting or release of this budget. |
| `description`          | String  | No       | Optional long-form explanation of the budget's purpose, constraints, and intended usage.             |

* **Notes:**
  - A `budget_definition` section should appear only once per budgeted pool within a proposal.
  - Hosts MUST validate that `token_type` corresponds to a known or registered token/resource.
  - The `total_amount` serves as the maximum allocatable sum across any associated `allocation_request` sections.
  - Hosts MAY enforce that all `allocation_request` sub-sections total less than or equal to `total_amount`.
  - If `recurring` is set to `true`, `budget_period_days` MUST be specified.

* **Example JSON Snippet:**

```json
{
  "kind": "budget_definition",
  "title": "Core Ops Budget – Q1",
  "props": {
    "budget_name": "core_ops_q1",
    "token_type": "icn::nfr",
    "total_amount": 50000,
    "budget_period_days": 90,
    "recurring": false,
    "description": "Funds for operational expenses and role-based compensation in Q1."
  },
  "sections": [
    {
      "kind": "allocation_request",
      "title": "Engineering Allocation",
      "props": {
        "amount": 30000,
        "recipient_role": "engineer",
        "justification": "Implementation of approved roadmap items"
      }
    },
    {
      "kind": "allocation_request",
      "title": "Governance Allocation",
      "props": {
        "amount": 20000,
        "recipient_role": "council_member",
        "justification": "Quarterly governance activities and outreach"
      }
    }
  ]
}
```
---

---

</rewritten_file>

--- 