---
id: global_development
title: Global Development Rules

applies_to:
  - all-languages
  - all-platforms

scope:
  - global

type: rule

priority: 100

always_include: true

agent_only: false

examples: false

tags:
  - conventions
  - solid
  - error-handling
  - security
  - performance
---

# Global Development Rules

Baseline rules that apply to every codebase and language, regardless of platform. Platform-specific rule files extend these, they never override them.

---

## Architecture & Code Quality

- Follow SOLID principles; prefer composition over inheritance.
- Keep functions small and single-purpose; one clear responsibility per class/module.
- No dead code, commented-out blocks, or unused imports in final output.
- Do not introduce a new design pattern, library, or dependency not already used in the project without asking first.
- Match existing project conventions (naming, folder structure, formatting) over personal preference.

---

## Error Handling

- Never fail silently — catch errors explicitly and handle or propagate them with context.
- Avoid using `undefined`/implicit optionals as a substitute for proper error states; use explicit types (`Result`, `Optional`, typed exceptions).
- Validate all external input (network, user, file) before use.
- Log errors with enough context to debug, but never log secrets, tokens, or PII.

---

## Security

- Treat all external input as untrusted; sanitize and validate before use.
- Never hardcode secrets, API keys, or credentials — use environment variables or a secrets manager.
- Apply least-privilege access for any credentials, tokens, or roles created.
- Use parameterized queries; never build queries via raw string concatenation.

---

## Performance & Concurrency

- Avoid premature optimization, but never ignore obvious O(n²)+ hot paths on large data.
- Any concurrent/multi-threaded code must be reviewed for race conditions and proper scope/lifecycle management.
- Cache expensive or repeated computations/queries where it meaningfully helps.
- Prefer pagination/streaming over loading unbounded data into memory.

---

## Scope Discipline

- Only modify files directly relevant to the current task.
- Ask before touching files outside the current workspace/scope.
- If a required type, function, or file is missing from context, do not invent its implementation — leave a clear `// TODO` note or ask for it.

---

## Resources
- [SOLID Principles Overview](https://en.wikipedia.org/wiki/SOLID)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)