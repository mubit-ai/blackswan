from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Optional

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "examples" / "python" / "langgraph"))

from blackswan_langgraph_poc import (
    MemoryRoots,
    MemoryTopology,
    PromotableMemory,
    ROLE_ORDER,
    TurnRecord,
    run_langgraph_poc,
)


def _response_payload(
    *,
    summary: str,
    promotable_memories: Optional[list[dict[str, str]]] = None,
    targeted_feedback: Optional[list[dict[str, str]]] = None,
) -> str:
    return json.dumps(
        {
            "summary": summary,
            "promotable_memories": promotable_memories or [],
            "targeted_feedback": targeted_feedback or [],
        }
    )


class ScriptedAgentModel:
    def __init__(self, responses: dict[str, list[str]]) -> None:
        self._responses = {role: list(values) for role, values in responses.items()}

    async def generate(self, *, system: str, prompt: str) -> str:
        if "planning agent" in system.lower():
            role = "planner"
        elif "implementation agent" in system.lower():
            role = "implementer"
        else:
            role = "reviewer"

        values = self._responses[role]
        if len(values) == 1:
            return values[0]
        return values.pop(0)


class RuleBasedMemoryProvider:
    def __init__(self, *, fail_recall: bool = False) -> None:
        self.fail_recall = fail_recall

    async def __call__(self, messages: list[dict[str, str]], system: Optional[str]) -> str:
        prompt = messages[-1]["content"] if messages else ""
        if system and "selected_memories" in system:
            if self.fail_recall:
                return "not valid json"
            return json.dumps({"selected_memories": self._select_memories(prompt)})
        if system and '"actions"' in system:
            return json.dumps({"actions": []})
        return json.dumps({"actions": []})

    def _select_memories(self, prompt: str) -> list[str]:
        entries = re.findall(r"- ([^(\n]+?\.md) \(([^)]+)\)", prompt)
        if not entries:
            entries = [(filename, title) for title, filename in re.findall(r"- \[(.*?)\]\(([^)]+)\)", prompt)]
        if not entries:
            return []

        if "Planner output:" in prompt:
            prioritized = [
                filename
                for filename, title in entries
                if any(keyword in title.lower() for keyword in ("decision", "feedback", "contract"))
            ]
            if prioritized:
                return prioritized[:5]

        if "Implementer output:" in prompt:
            prioritized = [
                filename
                for filename, title in entries
                if "feedback" in title.lower() or "review" in title.lower()
            ]
            if prioritized:
                return prioritized[:5]

        prioritized = [
            filename
            for filename, title in entries
            if any(keyword in title.lower() for keyword in ("task brief", "constraint", "charter"))
        ]
        if prioritized:
            return prioritized[:5]

        return [filename for filename, _ in entries[:5]]


def _planner_response() -> str:
    return _response_payload(
        summary="Adopt a read-only GET /flags contract and keep the first release small.",
        promotable_memories=[
            {
                "name": "API shape contract",
                "description": "shared decision to keep a read-only GET /flags endpoint",
                "memory_type": "project",
                "content": "The first release is read-only and exposes GET /flags as the primary endpoint.",
            }
        ],
    )


def _implementer_response() -> str:
    return _response_payload(
        summary="Implement a small HTTP handler with explicit validation and pytest coverage.",
    )


def _reviewer_response() -> str:
    return _response_payload(
        summary="Return explicit 4xx/5xx responses and add regression tests for error paths.",
        targeted_feedback=[
            {
                "target_role": "implementer",
                "name": "Implementer error handling feedback",
                "description": "reviewer correction about explicit HTTP error handling",
                "content": "Avoid uncaught exceptions in handlers. Return explicit 4xx/5xx responses and cover them with pytest.",
            }
        ],
    )


def _default_responses() -> dict[str, list[str]]:
    return {
        "planner": [_planner_response()],
        "implementer": [_implementer_response()],
        "reviewer": [_reviewer_response()],
    }


def _turn(summary, role: str) -> TurnRecord:
    return next(turn for turn in summary.turn_records if turn.role == role)


@pytest.mark.asyncio
async def test_shared_decision_propagates_to_implementer_on_later_run(tmp_path: Path) -> None:
    provider = RuleBasedMemoryProvider()
    first_summary = await run_langgraph_poc(
        task_brief="Design a feature-flag API.",
        project_constraints=["Keep the first release read-only."],
        base_memory_dir=tmp_path,
        run_id="shared-propagation",
        memory_provider_factory=lambda _scope: provider,
        agent_model=ScriptedAgentModel(_default_responses()),
    )

    assert "api_shape_contract.md" in first_summary.promoted_shared

    second_summary = await run_langgraph_poc(
        task_brief="Design a feature-flag API.",
        project_constraints=["Keep the first release read-only."],
        base_memory_dir=tmp_path,
        run_id="shared-propagation",
        memory_provider_factory=lambda _scope: provider,
        agent_model=ScriptedAgentModel(_default_responses()),
    )

    assert "api_shape_contract.md" in _turn(second_summary, "implementer").shared_recalled_filenames


@pytest.mark.asyncio
async def test_reviewer_feedback_is_retained_in_implementer_local_memory(tmp_path: Path) -> None:
    provider = RuleBasedMemoryProvider()
    await run_langgraph_poc(
        task_brief="Design a feature-flag API.",
        project_constraints=["Return explicit HTTP errors."],
        base_memory_dir=tmp_path,
        run_id="local-feedback",
        memory_provider_factory=lambda _scope: provider,
        agent_model=ScriptedAgentModel(_default_responses()),
    )

    second_summary = await run_langgraph_poc(
        task_brief="Design a feature-flag API.",
        project_constraints=["Return explicit HTTP errors."],
        base_memory_dir=tmp_path,
        run_id="local-feedback",
        memory_provider_factory=lambda _scope: provider,
        agent_model=ScriptedAgentModel(_default_responses()),
    )

    assert "implementer_error_handling_feedback.md" in _turn(
        second_summary, "implementer"
    ).local_recalled_filenames


@pytest.mark.asyncio
async def test_shared_memory_writes_are_owned_by_seed_or_coordinator(tmp_path: Path) -> None:
    provider = RuleBasedMemoryProvider()
    summary = await run_langgraph_poc(
        task_brief="Design a feature-flag API.",
        project_constraints=["Keep the first release read-only."],
        base_memory_dir=tmp_path,
        run_id="shared-write-safety",
        memory_provider_factory=lambda _scope: provider,
        agent_model=ScriptedAgentModel(_default_responses()),
    )

    assert summary.shared_writes
    assert all(write.actor in {"seed", "coordinator"} for write in summary.shared_writes)
    assert not any(write.actor in ROLE_ORDER for write in summary.shared_writes)


@pytest.mark.asyncio
async def test_failed_recall_records_a_miss_without_stopping_graph(tmp_path: Path) -> None:
    provider = RuleBasedMemoryProvider(fail_recall=True)
    summary = await run_langgraph_poc(
        task_brief="Design a feature-flag API.",
        project_constraints=["Keep the first release read-only."],
        base_memory_dir=tmp_path,
        run_id="failed-recall",
        memory_provider_factory=lambda _scope: provider,
        agent_model=ScriptedAgentModel(_default_responses()),
    )

    assert [turn.role for turn in summary.turn_records] == list(ROLE_ORDER)
    assert any(turn.local_recall_miss for turn in summary.turn_records)
    assert any(turn.shared_recall_miss for turn in summary.turn_records)


@pytest.mark.asyncio
async def test_record_session_end_allows_shared_memory_to_resurface(tmp_path: Path) -> None:
    provider = RuleBasedMemoryProvider()
    roots = MemoryRoots.from_base(tmp_path, "session-hygiene")
    topology = await MemoryTopology.create(
        roots=roots,
        provider_factory=lambda _scope: provider,
    )

    try:
        await topology.seed_defaults(
            task_brief="Design a feature-flag API.",
            project_constraints=["Keep the first release read-only."],
        )
        await topology.promote_shared_memories(
            [
                PromotableMemory(
                    name="API shape contract",
                    description="shared decision to keep a read-only GET /flags endpoint",
                    memory_type="project",
                    content="The first release is read-only and exposes GET /flags.",
                    source_role="planner",
                )
            ]
        )

        first = await topology.shared_engine.recall("Planner output:\nFocus on the API contract.", [])
        second = await topology.shared_engine.recall("Planner output:\nFocus on the API contract.", [])
        await topology.record_session_end_all()
        third = await topology.shared_engine.recall("Planner output:\nFocus on the API contract.", [])
    finally:
        await topology.shutdown()

    first_files = {Path(memory.path).name for memory in first.memories}
    second_files = {Path(memory.path).name for memory in second.memories}
    third_files = {Path(memory.path).name for memory in third.memories}

    assert "api_shape_contract.md" in first_files
    assert "api_shape_contract.md" not in second_files
    assert "api_shape_contract.md" in third_files
