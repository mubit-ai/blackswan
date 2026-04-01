# Consolidation ("Dream" System)

Consolidation is blackswan's background garbage collector. It reviews, deduplicates, and reorganizes memories to keep the system healthy over time.

## How It Works

Consolidation is a gated process. It only runs when all four gates pass:

```
Time gate (24h since last run)
    ↓ pass
Scan throttle (10min since last check)
    ↓ pass
Session gate (5+ sessions since last run)
    ↓ pass
Lock gate (acquire PID lock)
    ↓ pass
→ Run consolidation
```

### Gate 1: Time Gate

At least `consolidation_cooldown` (default 24 hours) must have passed since the last successful consolidation. The last consolidation time is stored as the mtime of the `.consolidate-lock` file.

### Gate 2: Scan Throttle

Don't re-check the session count more than once every `consolidation_scan_throttle` (default 10 minutes). Prevents expensive rescans on every request.

### Gate 3: Session Gate

At least `consolidation_session_gate` (default 5) sessions must have completed since the last consolidation. Call `engine.record_session_end()` at the end of each session to increment the counter.

### Gate 4: Lock Gate

Acquire a PID-based filesystem lock (`.consolidate-lock`). If another process is already consolidating, this blocks. Stale locks (mtime > timeout AND dead PID) are automatically reclaimed.

## What Consolidation Does

When it runs, the consolidator:

1. Reads all memory files
2. Sends them to your LLM with a consolidation prompt
3. The LLM returns actions: merge, delete, or update
4. Actions are executed against the store

Typical actions:
- **Merge**: Two memories about the same topic → one combined memory
- **Delete**: Outdated or contradicted memories
- **Update**: Refresh stale descriptions or content

## Triggering Consolidation

### Automatic (recommended)

Call `record_session_end()` after each session. When enough sessions accumulate and the cooldown passes, consolidation runs automatically on the next check.

```rust
// At the end of a conversation session
engine.record_session_end().await;

// Check if consolidation should run (non-blocking background)
engine.consolidate_background().await;
```

### Manual

Force a consolidation run (ignores gates except the lock):

```rust
let ran = engine.consolidate().await?;
if ran {
    println!("consolidation completed");
} else {
    println!("consolidation was gated");
}
```

## PID Lock Details

The lock file (`.consolidate-lock`) contains the PID of the holder process.

**Acquisition**: Write PID → re-read → verify it's still your PID (last-writer-wins race resolution).

**Stale detection**: If the lock's mtime is older than `consolidation_lock_timeout` (default 60 minutes) AND the PID is dead (checked via `kill(pid, 0)` on Unix), the lock is reclaimed.

**Release**: The lock is held by an RAII guard (`PidLockGuard`). When the guard drops, the file is removed. If the process crashes, the stale detection handles cleanup.

**Rollback on failure**: If consolidation fails, the guard drops and removes the lock file. The session counter is not reset, so consolidation will be attempted again after the cooldown.

## Consolidation State

State is persisted in `.consolidation-state` (JSON):

```json
{
  "session_count": 3,
  "last_scan": "2026-04-01T10:00:00Z"
}
```

After successful consolidation, the session counter resets to 0.

## Tuning

| Goal | Change |
|------|--------|
| Consolidate more often | Lower `consolidation_session_gate` and `consolidation_cooldown` |
| Consolidate less often | Raise both values |
| Reduce LLM cost | Raise `consolidation_cooldown` to 48+ hours |
| Multi-process safety | The PID lock handles this — no extra config needed |

## Known Limitations

- **No atomic multi-file updates.** A crash mid-consolidation can leave partial state. The next run will clean up.
- **Non-deterministic results.** The LLM may make different merge/delete decisions each time. This is inherent.
- **200-file scan cap.** Projects with >200 memory files have invisible "ghost" memories. Consolidation should compact aggressively to stay under this cap.
