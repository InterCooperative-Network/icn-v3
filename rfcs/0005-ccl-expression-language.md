# RFC-0005: CCL Expression Language & Condition Evaluation

**Status:** Draft
**Author(s):** ICN Development Team, ICN AI Assistant
**Related:** RFC-0003 (CCL Context Model), RFC-0004 (Host ABI - Context Interface), ICN Canonical Types RFC
**Date:** (Current Date)

## 1. Introduction & Goals

This RFC defines the scope, grammar, and evaluation strategy for expressions within the Cooperative Contract Language (CCL). Expressions are fundamental to CCL, enabling conditional logic, data manipulation, and interaction with the execution context.

The primary goals for CCL's expression language and evaluation strategy are:

*   **Security:** Evaluation must occur within a sandboxed environment, preventing unintended side effects on the host or other contracts.
*   **Determinism & Auditability:** Expression evaluation must be fully deterministic. Given the same initial context and contract state, an expression must always yield the same result, ensuring reproducibility and verifiability for replays and audits.
*   **Clarity & Readability:** The expression syntax should be intuitive for contract authors and easy to understand.
*   **Performance:** While not sacrificing security or clarity, expression evaluation should be reasonably efficient.
*   **Portability:** The model should be implementable across different ICN runtime environments that support WebAssembly (WASM).

This RFC recommends a **WASM-Side Evaluation** strategy, where CCL expressions are compiled into WASM instructions that operate on data fetched from the execution context.

## 2. Expression Scope & Grammar

CCL expressions are designed to be side-effect-free and evaluate to a single value. The following constructs are supported:

### 2.1. Literals

*   **Numbers:** Integer (e.g., `123`, `-42`) and potentially fixed-point decimal numbers. (Exact numeric types to be detailed in Canonical Types RFC). For this RFC, we assume `Number` can represent integers.
*   **Strings:** UTF-8 encoded strings, enclosed in double quotes (e.g., `"hello world"`, `"did:icn:example"`). Special characters are escaped using standard conventions (e.g., `\n`, `\t`, `\"`).
*   **Booleans:** `true` and `false`.
*   **Null:** `null`, representing an absent or undefined value.
*   **Arrays (Conceptual):** Literal array definitions (e.g., `[1, 2, 3]`) might be supported for constructing data within expressions, primarily for passing to stdlib functions. (To be detailed in CCL Language Spec).
*   **Objects (Conceptual):** Literal object definitions (e.g., `{ "key": "value" }`) might be supported. (To be detailed in CCL Language Spec).

### 2.2. Context Path Access

*   Access to data within the execution context (defined in RFC-0003) using dot-notation.
*   Examples: `caller.id`, `trigger.parameters.amount`, `system.timestamp_unix`.
*   Access semantics (e.g., behavior for missing paths yielding `null`) are defined in RFC-0003.

### 2.3. Operators

CCL supports standard operators with conventional precedence and associativity.

*   **Arithmetic Operators:**
    *   `+` (addition, string concatenation)
    *   `-` (subtraction)
    *   `*` (multiplication)
    *   `/` (division) - Behavior for division by zero must be defined (e.g., trap).
    *   `%` (modulo) - Behavior for modulo by zero must be defined.
    *   *Type safety: These operators expect numeric operands (except `+` which also supports string concatenation). Operations on mismatched types (without explicit conversion) will trap, as per RFC-0003.*
*   **Comparison Operators:**
    *   `==` (equality)
    *   `!=` (inequality)
    *   `<` (less than)
    *   `>` (greater than)
    *   `<=` (less than or equal to)
    *   `>=` (greater than or equal to)
    *   *Type safety: Comparison between different types (e.g., Number and String) results in `false` for `==` and `true` for `!=`, and traps for order comparisons. Comparisons involving `null` are well-defined (e.g., `null == null` is `true`; `null == <non-null>` is `false`).*
*   **Logical Operators:**
    *   `&&` (logical AND) - Short-circuiting.
    *   `||` (logical OR) - Short-circuiting.
    *   `!` (logical NOT).
    *   *Operands are coerced to boolean where necessary (e.g., `null`, `0`, `""` are falsy; others truthy – exact rules to be defined).*
*   **Membership Operator (Conceptual - for Arrays):**
    *   `in` (e.g., `my_value in array_variable`, `"admin" in caller.roles`).
    *   Checks for the presence of an element within an array.

### 2.4. CCL Standard Library Function Calls

Expressions can include calls to side-effect-free functions from the CCL standard library. These functions provide common utilities, type checks, and safe operations.

*   **Essential Examples (Illustrative - full list in Standard Library RFC):**
    *   Type Checking: `is_string(value)`, `is_number(value)`, `is_boolean(value)`, `is_array(value)`, `is_object(value)`, `is_null(value)`.
    *   Safe Type Conversions: `try_to_string(value)`, `try_to_number(value)`. (Returning a result object or `null` on failure).
    *   String Manipulation: `string_length(string_value)`, `concat(...)` (if not an operator), `substring(string_value, start, length)`.
    *   Array Manipulation: `array_length(array_value)`, `array_contains(array_value, element_value)` (alternative to `in` operator).
    *   Path Navigation (Safe Get): `get_path(object_value, "path.to.nested.value")` - Returns the value at the path or `null` if the path is invalid or not found, preventing errors from raw dot access on potentially `null` intermediate paths.
    *   Context Access Helpers (if needed beyond direct path access): e.g., `get_trigger_param("param_name")`.

### 2.5. Precedence and Associativity

Standard operator precedence rules (e.g., `*`/`/` before `+`/`-`, `&&` before `||`) will apply. Parentheses `()` can be used to explicitly control the order of evaluation.

## 3. Evaluation Strategy

This RFC proposes that CCL expression evaluation is performed **entirely within the WASM guest environment (WASM-Side Evaluation)**.

### 3.1. Option A: Host-Side Evaluation (Considered and Rejected)

*   **Mechanism:** The CCL compiler would emit a representation of the expression (e.g., the raw expression string or a serialized AST). This representation would be passed to a host ABI function (e.g., `host_evaluate_condition(expression_data)`), which would parse and evaluate it, returning the result.
*   **Pros:**
    *   Could potentially simplify the CCL compiler if it offloads complex expression parsing and evaluation.
    *   The host has direct access to all context data without needing FFI calls for each part.
*   **Cons:**
    *   **Security Risks:** Passing arbitrary strings or complex structures from WASM to the host for evaluation creates a significant attack surface. Malformed or malicious expressions could exploit vulnerabilities in the host's parser or evaluator.
    *   **Reduced Transparency:** The evaluation logic resides within the host, making it less transparent and harder to audit directly from the WASM bytecode.
    *   **Increased Host ABI Complexity:** Requires the host to implement a robust and secure expression evaluation engine, which is a substantial undertaking.
    *   **Performance Bottlenecks:** FFI call overhead for every condition evaluation could be significant.
    *   **Inconsistent Type System:** Potential for mismatches or subtle differences between the CCL type system (within WASM) and the host's interpretation of types within expressions.

### 3.2. Option B: WASM-Side Evaluation (Recommended)

*   **Mechanism:** The CCL compiler is responsible for translating CCL expressions into a sequence of WASM instructions. This involves:
    1.  **Data Fetching:** Generating code to access relevant values from the execution context. This primarily involves reading from the eager context block (delivered as per RFC-0004) using memory offsets and lengths (`__ICN_CONTEXT_START`, `__ICN_CONTEXT_LEN`), and deserializing parts of it as needed (e.g., CBOR parsing within the WASM module, potentially aided by stdlib). If necessary, for very large or sensitive data not in the eager block, it might involve calls to specific, narrowly-defined host ABI functions like `host_get_context_value` (as defined in RFC-0004, if implemented by the host).
    2.  **Operations:** Using native WASM instructions for basic arithmetic, logical, and comparison operations.
    3.  **Standard Library Calls:** Generating calls to CCL standard library functions (also compiled to WASM) for more complex operations, type checks, safe conversions, string/array manipulations, etc.
    4.  **Control Flow:** Utilizing WASM's native conditional branching instructions (e.g., `if`, `br_if`) based on the results of evaluated expressions.
*   **Pros:**
    *   **Enhanced Security:** All evaluation logic is sandboxed within the WASM guest environment. The host's attack surface is minimized as it is primarily responsible for data provision and action execution, not arbitrary expression evaluation.
    *   **Improved Transparency & Auditability:** The complete evaluation logic is part of the compiled WASM bytecode, making it fully auditable and analyzable.
    *   **Simplified Host ABI:** The host ABI remains focused on data delivery (RFC-0004) and executing specific, well-defined actions (e.g., `host_anchor_data`, `host_mint_token`). No complex `host_evaluate_condition` function is needed.
    *   **Consistent Type System:** Evaluation occurs within the CCL/WASM type system, leveraging the type safety rules and stdlib functions defined in RFC-0003 and the upcoming Standard Library RFC.
    *   **Potential for Optimization:** The CCL compiler can perform optimizations on the generated WASM code for expressions.
*   **Cons:**
    *   **Increased CCL Compiler Complexity:** The compiler needs to be capable of parsing expressions and generating the corresponding WASM instruction sequences and stdlib calls. This is, however, a standard responsibility for language compilers.
    *   **Potential FFI Overhead for Granular Data (Mitigated):** If an expression requires many disparate pieces of context data not present in an efficiently accessible eager block, it could lead to multiple FFI calls if `host_get_context_value` is used heavily. RFC-0004's proposal for a comprehensive eager context block aims to mitigate this for most common cases.

**This RFC formally recommends the adoption of Option B: WASM-Side Evaluation.**

## 4. Impact on Host ABI & Opcodes

Adopting WASM-Side Evaluation has the following implications:

*   **Removal of `host_evaluate_condition` ABI:** The conceptual host ABI function `host_evaluate_condition(expression_string)` is **NOT REQUIRED** and should **NOT** be implemented as part of the host ABI for CCL.
*   **No Specific Expression Evaluation Opcodes:** CCL will not define custom "expression evaluation" opcodes in the sense of a single opcode that takes an expression string. Instead, expressions are compiled into sequences of existing WASM instructions and calls to WASM-compiled standard library functions.
*   **Context Data Access:**
    *   The primary mechanism for accessing context data for expressions is by reading and parsing the eager context block (from `__ICN_CONTEXT_START`, `__ICN_CONTEXT_LEN`) within the WASM module.
    *   The optional `host_get_context_value` ABI function (RFC-0004) can serve as a fallback for accessing very large, infrequently used, or sensitive context data not included in the eager block.
    *   The need for a more specialized, highly optimized `Opcode::ContextGet(path_ptr, path_len)` that directly returns simple types or pointers *within the already loaded eager context block* could be considered if performance analysis shows significant overhead from stdlib-based parsing of the eager block for every access. However, efficient stdlib parsing/caching within WASM is the preferred initial approach.

## 5. Security & Determinism Guarantees

*   **Security:** By confining expression evaluation to the WASM sandbox, the risk of host-side vulnerabilities being exploited through expression evaluation is minimized. The host interface remains narrow.
*   **Determinism:** WASM-side evaluation, when combined with deterministic context data (RFC-0004) and side-effect-free standard library functions, ensures that expression evaluation is fully deterministic and reproducible.
*   **Type Safety:** Trapping on direct type-mismatched operations, as specified in RFC-0003, will be enforced by the compiled WASM code and the CCL standard library.

## 6. Recommendation & Next Steps

**Recommendation:** This RFC formally recommends the adoption of **WASM-Side Evaluation** for all CCL expressions.

**Next Steps:**

1.  **CCL Compiler Development:** The CCL compiler must be designed to parse the defined expression grammar (Section 2) and generate corresponding WASM instruction sequences and calls to the CCL standard library.
2.  **CCL Standard Library RFC:** A separate RFC will detail the CCL Standard Library, including the functions necessary to support expression evaluation (type checking, safe conversions, string/array ops, safe path navigation, etc.).
3.  **Canonical Types RFC:** Formalize the data types (Number, String, Boolean, Null, Array, Object, DID, Timestamp) used in CCL and its context model.
4.  **Detailed CCL Language Specification:** The full grammar, operator precedence, and detailed semantics of CCL expressions will be part of the comprehensive CCL language specification.

This approach provides a secure, transparent, and robust foundation for expression evaluation in CCL. 