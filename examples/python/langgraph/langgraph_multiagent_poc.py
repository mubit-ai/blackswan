#!/usr/bin/env python3
"""Run the LangGraph multiagent blackswan memory POC."""

from __future__ import annotations

import argparse
import asyncio
from typing import Optional, Sequence

from blackswan_langgraph_poc import format_console_summary, run_langgraph_poc


DEFAULT_TASK_BRIEF = (
    "Design a small Python web service for feature-flag evaluation. "
    "The team needs a planner, implementer, and reviewer to agree on the API shape, "
    "failure handling, and the tests required before shipping."
)

DEFAULT_CONSTRAINTS = [
    "Keep the first release read-only and avoid background workers.",
    "Return explicit error responses instead of uncaught exceptions.",
    "Prefer implementation details that are easy to test under pytest.",
]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--task-brief",
        default=DEFAULT_TASK_BRIEF,
        help="Primary task the three-agent team should work on.",
    )
    parser.add_argument(
        "--constraint",
        action="append",
        default=None,
        help="Additional shared project constraint. Can be repeated.",
    )
    parser.add_argument(
        "--model",
        default=None,
        help="OpenAI model to use. Falls back to OPENAI_MODEL if omitted.",
    )
    parser.add_argument(
        "--memory-dir",
        default=None,
        help="Base directory for POC memory runs. Falls back to BLACKSWAN_POC_MEMORY_DIR if omitted.",
    )
    parser.add_argument(
        "--run-id",
        default=None,
        help="Stable run identifier. Falls back to BLACKSWAN_POC_RUN_ID or a generated id.",
    )
    parser.add_argument(
        "--consolidate",
        action="store_true",
        help="Trigger background consolidation after the graph completes.",
    )
    return parser


async def amain(argv: Optional[Sequence[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    constraints = args.constraint if args.constraint is not None else list(DEFAULT_CONSTRAINTS)
    summary = await run_langgraph_poc(
        task_brief=args.task_brief,
        project_constraints=constraints,
        base_memory_dir=args.memory_dir,
        run_id=args.run_id,
        model=args.model,
        run_consolidation=args.consolidate,
    )
    print(format_console_summary(summary))
    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    return asyncio.run(amain(argv))


if __name__ == "__main__":
    raise SystemExit(main())
