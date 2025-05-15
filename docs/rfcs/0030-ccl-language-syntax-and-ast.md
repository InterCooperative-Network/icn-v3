# RFC 0030: CCL Language Syntax and Abstract Syntax Tree (AST)

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Governance Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0003 (CCL Context Model), RFC 0031 (CCL Compiler), RFC 0032 (Bylaws Templates)

---

## 0. Abstract

This RFC defines the syntax and AST structure for the **Cooperative Contract Language (CCL)** — ICN’s high-level language for encoding cooperative logic, governance proposals, and execution workflows. It outlines the token grammar, core expressions, type system, and semantic model.

---

## 1. Introduction

CCL enables cooperatives, communities, and federations to define enforceable, verifiable logic that can be executed as WASM. It is designed for:

* Expressing proposals, policies, and contracts
* Modeling scoped execution and dependencies
* Compiling to auditable, deterministic code

This RFC describes the human-readable syntax and the compiler-intermediate AST model.

---

## 2. Syntax Overview

CCL is a line-oriented, block-indented language. Key elements include:

```ccl
contract "Budget Increase" {
  let proposal = Proposal::new("Increase team budget", 5000)
  if proposal.approved_by("finance_committee") {
    treasury.mint(to: "dev_team", amount: 5000)
  }
}
```

### Comments

```ccl
# This is a comment
```

### Keywords

`contract`, `let`, `if`, `match`, `for`, `fn`, `true`, `false`, `return`

---

## 3. Token Types

The lexical grammar includes:

* **Identifiers**: `[a-zA-Z_][a-zA-Z0-9_]*`
* **Literals**: strings, integers, booleans
* **Operators**: `=`, `==`, `+`, `-`, `>`, `&&`, `||`, `!`
* **Delimiters**: `{`, `}`, `(`, `)`, `:`, `,`, `->`

---

## 4. AST Structure

The AST is produced by `icn-ccl-parser` and consumed by the compiler.

### 4.1 Contract

```rust
Contract {
  name: String,
  body: Vec<Statement>,
}
```

### 4.2 Statement

```rust
enum Statement {
  Let { name: String, value: Expression },
  If { condition: Expression, body: Vec<Statement> },
  Expression(Expression),
}
```

### 4.3 Expression

```rust
enum Expression {
  Literal(Literal),
  Variable(String),
  FunctionCall { name: String, args: Vec<Expression> },
  MethodCall { receiver: Box<Expression>, method: String, args: Vec<Expression> },
  BinaryOp { left: Box<Expression>, op: BinaryOperator, right: Box<Expression> },
}
```

### 4.4 Literals

```rust
enum Literal {
  Int(i64),
  Str(String),
  Bool(bool),
}
```

---

## 5. Semantics

Each contract:

* Executes within a scoped execution context
* Has access to host ABI calls (e.g., `treasury.mint`, `account.get_balance`)
* May fail early or short-circuit
* Is deterministic and side-effect isolated

---

## 6. Type System

* Statically typed during compilation
* Types include: `Int`, `Str`, `Bool`, `Void`
* Type inference supported for `let`
* Function/method signatures validated against host ABI registry

---

## 7. Error Model

Compile-time errors:

* Type mismatches
* Undeclared identifiers
* Invalid host method signatures

Runtime errors:

* Insufficient permissions
* Host rejection (e.g., quota exceeded)
* Execution limit violations

---

## 8. Observability

Compiler includes:

* Source-to-AST visualizer (planned)
* Span-based error messages
* Optionally emits `.ast.json` for debugging

---

## 9. Rationale and Alternatives

The current syntax is designed for:

* Human readability
* Compatibility with WASM backend
* Auditability and source mapping

Alternatives like embedded scripting (e.g. Lua) were rejected due to trust, tooling, and compilation limitations.

---

## 10. Backward Compatibility

CCL v1 syntax is stable. This RFC documents its grammar and AST. Compiler and runtime integrations (RFC 0031) are forward-compatible.

---

## 11. Open Questions and Future Work

* Support for algebraic data types or enums?
* Higher-order functions or closures?
* Scope-aware imports or module system?

---

## 12. Acknowledgements

Thanks to the authors of `icn-ccl-parser` and the contract governance test suite for validating early language models.

---

## 13. References

* \[RFC 0031: CCL Compiler and WASM Generation (planned)]
* \[RFC 0003: CCL Context and Scope Model]
* \[RFC 0032: Bylaws and Voting Templates (planned)]

---

**Filename:** `0030-ccl-language-syntax-and-ast.md`
