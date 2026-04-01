# Memory Types

blackswan uses a closed 4-type taxonomy. Every memory must be classified into exactly one type.

## Overview

| Type | What to Store | Structure |
|------|--------------|-----------|
| `user` | Role, goals, preferences, expertise, collaboration style | Free-form description |
| `feedback` | Corrections, confirmed approaches | Rule, then **Why:** and **How to apply:** |
| `project` | Initiatives, deadlines, decisions, who is doing what | Fact/decision, then **Why:** and **How to apply:** |
| `reference` | Pointers to external systems (trackers, dashboards, docs) | System name + URL/path + purpose |

## User Memories

Store information about who the user is and how to work with them.

```markdown
---
name: user is senior Rust engineer
description: user has 5+ years of Rust experience, prefers idiomatic patterns
type: user
---

Senior backend engineer specializing in Rust. Has deep expertise in async, 
trait design, and systems programming. Prefers idiomatic Rust patterns over 
"translating" from other languages.
```

**Good user memories:**
- "User is a data scientist investigating logging" — tailors explanations to their background
- "User has deep Go expertise, new to React" — frames frontend in backend analogues
- "User prefers terse responses, no trailing summaries" — adjusts output style

**Bad user memories:**
- Personality assessments or judgments
- Information the user didn't share
- Opinions about the user's code quality

## Feedback Memories

Store corrections and confirmed approaches. These are the most impactful memory type — they prevent the agent from repeating mistakes.

```markdown
---
name: integration tests must use real database
description: no mocking the database in integration tests — prior incident with mock/prod divergence
type: feedback
---

Integration tests must hit a real database, not mocks.

**Why:** Last quarter, mocked tests passed but the production migration failed because 
the mock didn't reflect actual constraint behavior. The team lost two days debugging.

**How to apply:** When writing or modifying integration tests in the `tests/` directory, 
always connect to the test database. Only use mocks for unit tests of pure business logic.
```

**Good feedback memories:**
- Corrections: "don't mock the database", "stop summarizing at the end"
- Confirmations: "yes, the single bundled PR was right here"
- Approach preferences: "always run clippy before committing"

**Bad feedback memories:**
- One-off task instructions ("use port 8080 for this test")
- Debugging steps for a specific incident

## Project Memories

Store ongoing initiatives, deadlines, and decisions that aren't derivable from code or git.

```markdown
---
name: merge freeze for mobile release
description: no non-critical merges after 2026-03-05 — mobile team cutting release branch
type: project
---

Merge freeze begins 2026-03-05 for the mobile release cut.

**Why:** The mobile team needs a stable base branch. Last release was delayed 
by a late merge that broke the build.

**How to apply:** Flag any non-critical PR work scheduled after March 5. 
Prioritize getting in-progress work merged before the freeze.
```

**Good project memories:**
- Deadlines with context: "API v3 ships 2026-04-15, contractual"
- Architecture decisions with reasoning: "chose SQLite over Postgres for the edge service because..."
- Who is doing what: "Sarah owns the auth rewrite, driven by compliance"

**Bad project memories:**
- Code patterns or file structure (read the code)
- Git history (use `git log`)
- Anything already in a CLAUDE.md or project docs

**Important:** Always convert relative dates to absolute. "Next Thursday" should be stored as "2026-04-03".

## Reference Memories

Store pointers to where information lives in external systems.

```markdown
---
name: pipeline bugs in Linear
description: bug tracking for data pipeline is in Linear project "INGEST"
type: reference
---

Pipeline bugs are tracked in Linear project "INGEST".
URL: https://linear.app/company/project/ingest

Used by the data engineering team. Check here before investigating 
pipeline-related issues — there may be known issues or workarounds.
```

**Good reference memories:**
- "Grafana dashboard for API latency: grafana.internal/d/api-latency"
- "Design docs live in the /docs folder on Notion"
- "CI/CD is configured in .github/workflows, but secrets are in Vault"

**Bad reference memories:**
- The content of those systems (it changes; store the pointer, not the snapshot)

## Hard Exclusions

Never save as memory, regardless of type:

- **Code patterns, architecture, file paths** — derivable by reading the codebase
- **Git history, recent changes, who-changed-what** — `git log` is authoritative
- **Debugging solutions or fix recipes** — the fix is in the code, the commit message has context
- **Anything already in project instruction files** — don't duplicate
- **Ephemeral task details** — current conversation state, temporary debugging info
