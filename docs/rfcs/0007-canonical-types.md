# RFC-0007: ICN Canonical Data Types

**Status:** Draft
**Author(s):** ICN Development Team, ICN AI Assistant
**Related:** RFC-0003 (CCL Context Model), RFC-0004 (Host ABI - Context Interface), RFC-0005 (CCL Expression Language), RFC-0006 (CCL Standard Library)
**Date:** (Current Date)

---

## 1. Introduction

This RFC defines the canonical data types used across the InterCooperative Network (ICN) platform. These types are foundational for the Cooperative Contract Language (CCL), the Host ABI, the execution context (RFC-0003), the CCL Standard Library (RFC-0006), data serialization, and storage. Adherence to these types ensures consistency, interoperability, and clarity across all ICN components and specifications.

For each type, this document specifies:
- Its base type (if applicable, from primitives or other defined types).
- Its semantic meaning and intended use.
- Its canonical representation, particularly for serialization across the Host ABI, in DAG entries, or for storage.
- Any relevant validation rules, constraints (e.g., regex patterns, enum values), or specific formatting.

---

## 2. Primitive Types

These are the fundamental building blocks for all data within ICN.

### 2.1. `Null`
- **Base Type:** N/A (Intrinsic type)
- **Meaning:** Represents the intentional absence of a value or an undefined state.
- **Canonical Representation:**
    - JSON: `null`
    - CBOR: Major Type 7, Simple Value 22
- **Comparison:** In CCL expressions (as per RFC-0005), `null == null` evaluates to `true`. Comparison of `null` with any non-`null` value using `==` evaluates to `false`. Order comparisons (`<`, `>`, `<=`, `>=`) involving `null` typically trap or are ill-defined, depending on the specific operator semantics in CCL.

### 2.2. `Boolean`
- **Base Type:** N/A (Intrinsic type)
- **Values:** `true`, `false`
- **Meaning:** Represents a logical truth value.
- **Canonical Representation:**
    - JSON: `true`, `false`
    - CBOR: Major Type 7, Simple Values 21 (`true`) and 20 (`false`)
- **Coercion:** See `try_to_boolean()` in RFC-0006 for CCL coercion rules.

### 2.3. `Number`
- **Base Type:** 64-bit Integer.
- **Meaning:** Represents numerical values.
- **Canonical Forms & Usage:**
    - **`i64` (Signed 64-bit Integer):** The preferred canonical form for general arithmetic operations within CCL and for representing countable quantities that can be negative.
    - **`u64` (Unsigned 64-bit Integer):** Used for specific domain values where non-negativity is inherent and the full positive range is beneficial (e.g., `Timestamp`, `block_height`, lengths, counts).
    - CCL's type system will distinguish between these or provide clear coercion rules where necessary. The CCL standard library (RFC-0006) will provide functions that operate on these specific integer types.
- **Precision:** Floating-point numbers are explicitly **excluded** from this core `Number` type to ensure deterministic arithmetic and avoid precision issues common in financial or consensus-critical calculations. Support for fixed-point decimal arithmetic may be proposed in a future RFC if required for specific economic models.
- **Canonical Representation:**
    - JSON: `Number` (standard JSON number, integral values)
    - CBOR: Major Types 0 (unsigned integer) and 1 (negative integer)
- **Comparison:** Standard numerical comparisons. Comparisons between `Number` and `String` (or other types) trap or follow specific rules defined in RFC-0005.

### 2.4. `String`
- **Base Type:** N/A (Intrinsic type)
- **Meaning:** Represents a sequence of characters.
- **Encoding:** **UTF-8**. All strings exchanged via ABI, stored, or processed by CCL **MUST** be valid UTF-8.
- **Maximum Length:** While theoretically unbounded, practical limits may be imposed by specific host implementations, ABI buffer sizes, or application-level constraints. Such limits should be documented where they apply.
- **Canonical Representation:**
    - JSON: `String`
    - CBOR: Major Type 3 (UTF-8 string)
- **Usage:** Default type for textual data, identifiers, paths, DIDs, roles, etc.

---

## 3. Composite Types

These types are constructed from primitive types or other composite types.

### 3.1. `Array`
- **Base Type:** Ordered, heterogeneous list of values.
- **Elements:** Each element can be of `Any` CCL canonical type (i.e., `Null`, `Boolean`, `Number`, `String`, `Array`, or `Object`).
- **Indexing:** 0-based integer indexing.
- **Canonical Representation:**
    - JSON: `Array`
    - CBOR: Major Type 4 (Array of data items)
- **Length:** Can be determined by `array_length()` (RFC-0006).

### 3.2. `Object`
- **Base Type:** Key-value map; an unordered collection of key-value pairs.
- **Key Type:** **`String`**. Keys within an object **MUST** be unique.
- **Value Type:** Each value can be of `Any` CCL canonical type.
- **Canonical Representation:**
    - JSON: `Object`
    - CBOR: Major Type 5 (Map of pairs of data items)
- **Constraints:** Keys must be unique. Objects can be nested. Order of keys is generally not significant, though some serialization formats might preserve it.

---

## 4. Semantic Types (Type Aliases with Constraints)

These types are based on one of the primitive or composite types but carry additional semantic meaning and are often subject to specific formatting or validation rules. Compilers and runtimes should, where feasible, enforce these constraints.

### 4.1. `DID` (Decentralized Identifier)
- **Base Type:** `String`
- **Meaning:** A globally unique identifier for an entity, conforming to W3C DID specifications.
- **Format:** MUST conform to the DID Core specification syntax. For ICN, DIDs typically use the `did:icn:` method prefix, e.g., `did:icn:z6MkpP5t3gKWfxS4bXq8x...`
- **Validation:** Regex: `^did:[a-z0-9]+:[a-zA-Z0-9:._%-]+$` (General DID syntax). Specific ICN method validation: `^did:icn:[a-zA-Z0-9:._%-]+$`.
- **Usage:** Identity of callers, issuers, recipients, contract owners, etc.

### 4.2. `Timestamp`
- **Base Type:** `Number` (specifically `u64`)
- **Meaning:** Represents a point in time.
- **Unit:** Unix Epoch Time (seconds since 1970-01-01T00:00:00Z, Coordinated Universal Time - UTC).
- **Usage:** Primary timestamp for `system.timestamp_unix`, `trigger.timestamp_unix` (RFC-0003).
- **Coercion/Formatting:** No timezone information is stored with the timestamp itself. The CCL standard library (RFC-0006) provides functions for formatting this `u64` value into human-readable string representations (e.g., ISO 8601 / RFC 3339).

### 4.3. `ExecutionMode`
- **Base Type:** `String` (Enum)
- **Meaning:** Indicates the mode in which CCL is currently executing (see `system.execution_mode` in RFC-0003).
- **Allowed Values (Case-sensitive):**
    - `"Genesis"`
    - `"Normal"`
    - `"Replay"`
    - `"Query"`
- **Validation:** Value MUST be one of the defined enum strings.

### 4.4. `ContractStatus`
- **Base Type:** `String` (Enum)
- **Meaning:** Represents the current execution status of a CCL contract instance (see `contract.status` in RFC-0003).
- **Allowed Values (Case-sensitive):**
    - `"Initializing"`
    - `"Running"`
    - `"Succeeded"`
    - `"Failed"`
    - (Potentially others like "Paused", "Upgrading" - to be expanded if necessary)
- **Validation:** Value MUST be one of the defined enum strings.

### 4.5. `ResourceTypeName`
- **Base Type:** `String` (Enum)
- **Meaning:** Identifies a type of resource for economic tracking and metering (see `icn-economics` and Chapter 7 examples).
- **Allowed Values (Case-sensitive):**
    - `"CPU"` (Computation units)
    - `"Memory"` (Memory usage)
    - `"Network"` (Network bandwidth/IO)
    - `"Storage"` (Persistent storage usage)
    *   `"Transaction"` (A general cost for submitting/processing a transaction)
    *   `"Custom"` (For application-defined metered resources)
- **Validation:** Value MUST be one of the defined enum strings or follow a defined pattern for "Custom" extensions if allowed.

---

## 5. Naming and Identifier Types (Semantic Strings)

These types are specialized strings used for various identifiers within CCL and the ICN system.

### 5.1. `CCLIdentifier`
- **Base Type:** `String`
- **Meaning:** Names used for variables, functions, parameters, and keys within CCL code and context objects.
- **Constraints:** MUST match the regex `^[a-zA-Z_][a-zA-Z0-9_]*$`. (Starts with a letter or underscore, followed by letters, numbers, or underscores). Keywords of the CCL language are reserved.
- **Validation:** Enforced by the CCL compiler and runtime when parsing context keys.

### 5.2. `ContextPath`
- **Base Type:** `String`
- **Meaning:** A dot-separated path used to navigate `Object` structures, particularly within the execution context (e.g., for `get_path()` in RFC-0006 or `host_get_context_value` in RFC-0004).
- **Constraints:** Consists of one or more `CCLIdentifier` segments joined by `.` (dot) characters (e.g., `trigger.parameters.amount`). No leading/trailing dots, no consecutive dots.
- **Validation:** Parsed by functions that use these paths.

### 5.3. Other Common Naming Aliases (Semantic Strings)

These aliases are based on `String` and imply a specific domain of use. They generally adhere to `CCLIdentifier` rules or may have slightly broader character sets if they represent external names. Specific validation rules (e.g., length, character subset) may apply on a case-by-case basis depending on their use.

| Name                | Description                                     | Example Constraints (Illustrative)             |
|---------------------|-------------------------------------------------|------------------------------------------------|
| `RoleName`          | Name of a role in `roles_def`                   | Typically `CCLIdentifier`, e.g., "treasurer"     |
| `PermissionName`    | Identifier for a permission                     | Dot-separated `CCLIdentifier`s, e.g., "budget.allocate" |
| `BudgetName`        | Name of a `budget_def`                          | Typically `CCLIdentifier` or human-readable string |
| `CategoryName`      | Name of a category within a `budget_def`        | Typically `CCLIdentifier`, e.g., "operations"  |
| `EventTypeName`     | Identifier for an event type in `trigger.type`  | Dot-separated `CCLIdentifier`s, e.g., "proposal.vote.cast" |
| `DefinitionName`    | Name for a CCL definition (proposal, process)   | Human-readable string, often with versioning |

---

## 6. Serialization Guidelines

For all canonical types, when data is exchanged across the Host ABI, serialized into DAG entries, or stored persistently:

*   **CBOR (Concise Binary Object Representation)** is the **preferred** canonical serialization format due to its compactness and schema-less flexibility.
*   **Canonical JSON** is an **acceptable alternative** for human-readable outputs, logging, or interfacing with systems where CBOR is not readily available. If JSON is used, numbers should be represented without loss of precision (large integers may need to be stringified if the JSON parser/target system cannot handle 64-bit integers natively, though this is an application-level concern).
*   **Enum-like strings** (e.g., `ExecutionMode`, `ResourceTypeName`) **MUST** match the defined values exactly (case-sensitive).
*   When serializing `Object` types, omitting keys whose values are `null` is permissible and often preferred to reduce payload size, unless the explicit presence of the key with a `null` value is semantically significant for the application. This behavior should be consistent.
*   Detailed binary serialization specifications for complex types or specific ABI interactions will be provided in the relevant Host ABI RFC appendices or related interface definitions.

---

## 7. Future Extensions

This RFC establishes the foundational set of canonical data types. Future proposals may extend this set to include:

*   **Fixed-Point Decimal Numbers:** For precise financial calculations if the base `Number` (integer) type proves insufficient for certain economic models.
*   **Duration Type:** For representing time durations with specific units (e.g., "10days", "2hours").
*   **Versioned Schema Identifiers/Tags:** To allow for explicit versioning of data structures.
*   **CID (Content Identifier) Type:** A formal type for CIDs, based on `String` or bytes, with validation for CID formatting, to represent links to data on IPFS or other content-addressed systems.
*   **Byte Array Type:** For handling raw binary data.

---

## 8. Recommendation

This RFC establishes the core type system for the ICN platform and CCL. All existing and future specifications, including but not limited to the Host ABI, CCL standard library functions, execution context schemas, and data storage formats, **MUST** reference these canonical types to ensure clarity, consistency, and interoperability across the entire ICN ecosystem. Any deviation or new type proposal should be made via a separate RFC or an amendment to this one. 