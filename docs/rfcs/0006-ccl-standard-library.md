### 3.2. Safe Type Conversion

These functions attempt to convert a value to a target type, returning `null` on failure.

| Function           | Signature                       | Description                                                                                                     |
|--------------------|---------------------------------|-----------------------------------------------------------------------------------------------------------------|
| `try_to_string`    | `(value: Any) -> String?`       | Attempts to convert `value` to a String. Returns the string representation or `null` if conversion is not possible or if the input `value` is `null`. Numbers and Booleans are converted to their standard string forms. |
| `try_to_number`    | `(value: Any) -> Number?`       | Attempts to convert `value` (e.g., a String) to a Number. Returns the number or `null` if conversion fails or if the input `value` is `null`. Boolean `true` converts to `1`, `false` to `0`. |
| `try_to_boolean`   | `(value: Any) -> Boolean?`      | Attempts to convert `value` to a Boolean using truthiness/falsiness rules (e.g., `0`, `""`, empty arrays/objects are `false`; others `true`). Returns `null` if the input `value` is `null` or if the input type is fundamentally non-coercible in a way that doesn't align with truthiness/falsiness (e.g., an opaque object with no defined truthiness). |

*Note: `?` indicates the return type can be `Null`. Inputting `null` to any `try_to_X` function will result in `null` output.*

### 3.6. Utility Functions

| Function      | Signature                                   | Description                                                                                     |
|---------------|---------------------------------------------|-------------------------------------------------------------------------------------------------|
| `coalesce`    | `(value1: Any, value2: Any, ...values) -> Any?` | Returns the first non-`null` argument. If all arguments are `null`, returns `null`.               |
| `generate_uuid` | `() -> String`                             | Returns a new, unique Version 4 UUID string. This function **MUST** call a dedicated host ABI function (e.g., `host_generate_uuid()`) which guarantees that the generated UUID is deterministic with respect to the current transaction/execution context, ensuring reproducibility during replays. This is a necessary exception to the general principle that standard library functions do not make host calls. The host is responsible for the deterministic generation mechanism (e.g., based on transaction ID, block hash, and an internal counter). |

## 4. Evaluation Semantics

## 5. Implementation and ABI

*   The CCL Standard Library functions are implemented in a language that compiles to WASM (e.g., Rust, C++) and are bundled with the compiled CCL contract.
*   They are linked at compile-time by the CCL compiler.
*   **Crucially, these standard library functions themselves do NOT directly make host ABI calls**, with the sole specified exception of `generate_uuid()` which relies on a host-provided deterministic UUID service for reproducibility. Their logic operates on data already in WASM memory (e.g., the eager context block, or values passed as arguments). 