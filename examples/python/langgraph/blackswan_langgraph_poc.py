"""LangGraph-based multiagent memory POC for blackswan."""

from __future__ import annotations

import inspect
import json
import os
import time
import uuid
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Mapping, Optional, Protocol, Sequence, TypedDict, Union

from blackswan import Memory, MemoryConfig, MemoryEngine, MemoryType, Message, MessageRole

try:
    from langgraph.graph import END, START, StateGraph
except ImportError:  # pragma: no cover - exercised in runtime environments without extras
    END = START = StateGraph = None

try:
    from langchain_openai import ChatOpenAI
except ImportError:  # pragma: no cover - exercised in runtime environments without extras
    ChatOpenAI = None

try:
    from openai import AsyncOpenAI
except ImportError:  # pragma: no cover - exercised in runtime environments without extras
    AsyncOpenAI = None


ROLE_ORDER = ("planner", "implementer", "reviewer")


@dataclass(slots=True)
class RoleSpec:
    role: str
    title: str
    focus: str
    system_prompt: str
    charter_name: str
    charter_description: str
    charter_content: str


ROLE_SPECS: dict[str, RoleSpec] = {
    "planner": RoleSpec(
        role="planner",
        title="Planning Agent",
        focus="Break the task into concrete implementation steps and surface durable project decisions.",
        system_prompt=(
            "You are the planning agent in a three-agent software delivery workflow. "
            "Turn the task brief into an implementation-ready plan. Prefer concrete decisions, "
            "scope boundaries, and dependencies that downstream agents can act on immediately."
        ),
        charter_name="planner charter",
        charter_description="planner role charter for the multiagent software delivery POC",
        charter_content=(
            "The planner is responsible for task decomposition, sequencing, shared decision capture, "
            "and identifying delivery risks."
        ),
    ),
    "implementer": RoleSpec(
        role="implementer",
        title="Implementation Agent",
        focus="Translate the plan into concrete code changes, interfaces, and tests.",
        system_prompt=(
            "You are the implementation agent in a three-agent software delivery workflow. "
            "Produce a concrete implementation approach with code-level detail, explicit interfaces, "
            "and the tests needed to validate the work."
        ),
        charter_name="implementer charter",
        charter_description="implementer role charter for the multiagent software delivery POC",
        charter_content=(
            "The implementer converts plans and shared decisions into executable changes, test plans, "
            "and concrete engineering tasks."
        ),
    ),
    "reviewer": RoleSpec(
        role="reviewer",
        title="Review Agent",
        focus="Identify regressions, missing tests, and reusable implementation feedback.",
        system_prompt=(
            "You are the reviewer agent in a three-agent software delivery workflow. "
            "Review the implementation critically, prioritize correctness risks, and produce reusable "
            "feedback that should influence future implementation turns."
        ),
        charter_name="reviewer charter",
        charter_description="reviewer role charter for the multiagent software delivery POC",
        charter_content=(
            "The reviewer inspects design and implementation outputs for bugs, regressions, testing gaps, "
            "and reusable feedback for later turns."
        ),
    ),
}


class AgentGraphState(TypedDict):
    run_id: str
    task_brief: str
    project_constraints: list[str]
    outputs: dict[str, str]
    parsed_outputs: dict[str, dict[str, Any]]
    turn_records: list[dict[str, Any]]
    promoted_shared: list[str]
    targeted_feedback: list[str]


class AgentModel(Protocol):
    async def generate(self, *, system: str, prompt: str) -> str:
        """Generate a text response for an agent turn."""


@dataclass(slots=True)
class MemoryRoots:
    run_dir: Path
    shared_dir: Path
    agent_dirs: dict[str, Path]

    @classmethod
    def from_base(cls, base_dir: Path, run_id: str) -> "MemoryRoots":
        run_dir = base_dir / run_id
        return cls(
            run_dir=run_dir,
            shared_dir=run_dir / "shared",
            agent_dirs={role: run_dir / "agents" / role for role in ROLE_ORDER},
        )

    def ensure(self) -> None:
        self.shared_dir.mkdir(parents=True, exist_ok=True)
        for path in self.agent_dirs.values():
            path.mkdir(parents=True, exist_ok=True)


@dataclass(slots=True)
class PromotableMemory:
    name: str
    description: str
    memory_type: str
    content: str
    source_role: str


@dataclass(slots=True)
class TargetedFeedback:
    target_role: str
    name: str
    description: str
    content: str
    source_role: str


@dataclass(slots=True)
class ParsedAgentResponse:
    summary: str
    raw_output: str
    promotable_memories: list[PromotableMemory] = field(default_factory=list)
    targeted_feedback: list[TargetedFeedback] = field(default_factory=list)


@dataclass(slots=True)
class TurnRecord:
    role: str
    query: str
    output_summary: str
    local_recalled_filenames: list[str]
    shared_recalled_filenames: list[str]
    local_recall_miss: bool
    shared_recall_miss: bool
    latency_ms: int


@dataclass(slots=True)
class MemoryWriteRecord:
    scope: str
    target: str
    actor: str
    name: str
    filename: str
    memory_type: str


@dataclass(slots=True)
class ManifestSize:
    entry_count: int
    line_count: int
    byte_size: int


@dataclass(slots=True)
class ExpectationResult:
    name: str
    passed: bool
    detail: str


@dataclass(slots=True)
class PocRunSummary:
    run_id: str
    model: str
    run_dir: str
    task_brief: str
    project_constraints: list[str]
    started_at: str
    finished_at: str
    total_latency_ms: int
    turn_records: list[TurnRecord]
    promoted_shared: list[str]
    targeted_feedback: list[str]
    manifest_sizes: dict[str, ManifestSize]
    expectations: list[ExpectationResult]
    shared_writes: list[MemoryWriteRecord]
    agent_writes: list[MemoryWriteRecord]
    outputs: dict[str, str]
    summary_path: Optional[str] = None

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def _require_langgraph() -> None:
    if StateGraph is None or START is None or END is None:
        raise RuntimeError(
            "examples/python/langgraph/blackswan_langgraph_poc.py requires the optional POC dependencies and a Python 3.10+ "
            "runtime for LangGraph. "
            "Install them with `uv pip install -e './bindings/python[poc]'`."
        )


def _require_openai_memory_provider() -> None:
    if AsyncOpenAI is None:
        raise RuntimeError(
            "OpenAI support is not installed. Install the optional POC dependencies with "
            "`uv pip install -e './bindings/python[poc]'`."
        )


def _require_langchain_openai() -> None:
    if ChatOpenAI is None:
        raise RuntimeError(
            "langchain-openai is not installed. The POC path expects Python 3.10+ and the optional "
            "LangGraph/OpenAI extras. Install them with "
            "`uv pip install -e './bindings/python[poc]'`."
        )


def _utc_now() -> datetime:
    return datetime.now(timezone.utc)


def _slugify(name: str) -> str:
    collapsed: list[str] = []
    previous_underscore = False
    for char in name.lower():
        normalized = char if char.isalnum() else "_"
        if normalized == "_":
            if previous_underscore:
                continue
            previous_underscore = True
        else:
            previous_underscore = False
        collapsed.append(normalized)
    slug = "".join(collapsed).strip("_")
    return f"{slug}.md"


def _strip_json_fence(value: str) -> str:
    return (
        value.strip()
        .removeprefix("```json")
        .removeprefix("```")
        .removesuffix("```")
        .strip()
    )


def _coerce_message_text(response: Any) -> str:
    text_attr = getattr(response, "text", None)
    if isinstance(text_attr, str):
        return text_attr

    content = getattr(response, "content", "")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        chunks: list[str] = []
        for item in content:
            if isinstance(item, str):
                chunks.append(item)
            elif isinstance(item, Mapping):
                if isinstance(item.get("text"), str):
                    chunks.append(item["text"])
                elif item.get("type") == "text" and isinstance(item.get("content"), str):
                    chunks.append(item["content"])
        return "\n".join(chunk for chunk in chunks if chunk)
    return str(content)


def _coerce_openai_completion_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if content is None:
        return ""
    if isinstance(content, list):
        chunks: list[str] = []
        for item in content:
            if isinstance(item, str):
                chunks.append(item)
            elif isinstance(item, Mapping) and isinstance(item.get("text"), str):
                chunks.append(item["text"])
        return "\n".join(chunk for chunk in chunks if chunk)
    return str(content)


def _normalize_memory_type(value: Optional[str], default: str = "project") -> str:
    normalized = (value or default).strip().lower()
    if normalized in {"user", "feedback", "project", "reference"}:
        return normalized
    return default


def _memory_type_enum(value: str) -> MemoryType:
    normalized = _normalize_memory_type(value)
    if normalized == "user":
        return MemoryType.User
    if normalized == "feedback":
        return MemoryType.Feedback
    if normalized == "reference":
        return MemoryType.Reference
    return MemoryType.Project


def _memory_type_name(value: MemoryType) -> str:
    if value == MemoryType.User:
        return "user"
    if value == MemoryType.Feedback:
        return "feedback"
    if value == MemoryType.Reference:
        return "reference"
    return "project"


def _memory_filename(memory: Memory) -> str:
    if memory.path:
        filename = Path(memory.path).name
        if filename:
            return filename
    return _slugify(memory.name)


def _format_recalled_memories(memories: Sequence[Memory]) -> str:
    if not memories:
        return "None"

    blocks: list[str] = []
    for memory in memories:
        blocks.append(
            "\n".join(
                [
                    f"- Filename: {_memory_filename(memory)}",
                    f"  Name: {memory.name}",
                    f"  Type: {memory.memory_type}",
                    f"  Description: {memory.description}",
                    f"  Content: {memory.content.strip()}",
                ]
            )
        )
    return "\n".join(blocks)


def _parse_promotable_memories(raw_items: Any, source_role: str) -> list[PromotableMemory]:
    if not isinstance(raw_items, list):
        return []

    parsed: list[PromotableMemory] = []
    for index, item in enumerate(raw_items, start=1):
        if isinstance(item, str):
            content = item.strip()
            if not content:
                continue
            parsed.append(
                PromotableMemory(
                    name=f"{source_role} memory {index}",
                    description=f"durable fact surfaced by the {source_role}",
                    memory_type="project",
                    content=content,
                    source_role=source_role,
                )
            )
            continue

        if not isinstance(item, Mapping):
            continue

        name = str(item.get("name") or f"{source_role} memory {index}").strip()
        description = str(item.get("description") or f"durable fact surfaced by the {source_role}").strip()
        content = str(item.get("content") or "").strip()
        if not content:
            continue
        parsed.append(
            PromotableMemory(
                name=name,
                description=description,
                memory_type=_normalize_memory_type(str(item.get("memory_type") or item.get("type") or "project")),
                content=content,
                source_role=source_role,
            )
        )
    return parsed


def _parse_targeted_feedback(raw_items: Any, source_role: str) -> list[TargetedFeedback]:
    if not isinstance(raw_items, list):
        return []

    parsed: list[TargetedFeedback] = []
    for index, item in enumerate(raw_items, start=1):
        if not isinstance(item, Mapping):
            continue
        target_role = str(item.get("target_role") or "").strip().lower()
        if target_role not in ROLE_SPECS:
            continue
        name = str(item.get("name") or f"{source_role} feedback {index}").strip()
        description = str(item.get("description") or f"reusable feedback from {source_role}").strip()
        content = str(item.get("content") or "").strip()
        if not content:
            continue
        parsed.append(
            TargetedFeedback(
                target_role=target_role,
                name=name,
                description=description,
                content=content,
                source_role=source_role,
            )
        )
    return parsed


def parse_agent_response(raw_output: str, role: str) -> ParsedAgentResponse:
    normalized = _strip_json_fence(raw_output)
    try:
        payload = json.loads(normalized)
    except json.JSONDecodeError:
        return ParsedAgentResponse(summary=raw_output.strip(), raw_output=raw_output.strip())

    summary = str(payload.get("summary") or payload.get("deliverable") or raw_output).strip()
    return ParsedAgentResponse(
        summary=summary,
        raw_output=raw_output.strip(),
        promotable_memories=_parse_promotable_memories(payload.get("promotable_memories"), role),
        targeted_feedback=_parse_targeted_feedback(payload.get("targeted_feedback"), role),
    )


async def _maybe_aclose(resource: Any) -> None:
    close = getattr(resource, "aclose", None)
    if close is None:
        close = getattr(resource, "close", None)
    if close is None:
        return
    result = close()
    if inspect.isawaitable(result):
        await result


class OpenAIMemoryProvider:
    """Thin OpenAI adapter for the Python MemoryEngine binding."""

    def __init__(
        self,
        *,
        model: str,
        api_key: Optional[str] = None,
        base_url: Optional[str] = None,
        organization: Optional[str] = None,
    ) -> None:
        _require_openai_memory_provider()
        kwargs: dict[str, Any] = {"api_key": api_key or os.environ.get("OPENAI_API_KEY")}
        if base_url:
            kwargs["base_url"] = base_url
        if organization:
            kwargs["organization"] = organization
        self.model = model
        self.client = AsyncOpenAI(**kwargs)

    async def __call__(self, messages: list[dict[str, Any]], system: Optional[str]) -> str:
        api_messages: list[dict[str, Any]] = []
        if system:
            api_messages.append({"role": "system", "content": system})
        for message in messages:
            role = str(message.get("role") or "user")
            if role not in {"user", "assistant", "system"}:
                role = "user"
            api_messages.append(
                {
                    "role": role,
                    "content": str(message.get("content") or ""),
                }
            )

        response = await self.client.chat.completions.create(
            model=self.model,
            messages=api_messages,
        )
        return _coerce_openai_completion_text(response.choices[0].message.content)

    async def aclose(self) -> None:
        await _maybe_aclose(self.client)


class OpenAIAgentModel:
    """LangChain OpenAI adapter for agent reasoning turns."""

    def __init__(
        self,
        *,
        model: str,
        api_key: Optional[str] = None,
        temperature: float = 0.0,
        reasoning_effort: Optional[str] = None,
    ) -> None:
        _require_langchain_openai()
        kwargs: dict[str, Any] = {
            "model": model,
            "api_key": api_key or os.environ.get("OPENAI_API_KEY"),
            "temperature": temperature,
        }
        if reasoning_effort:
            kwargs["reasoning"] = {"effort": reasoning_effort, "summary": "auto"}
        self.model = ChatOpenAI(**kwargs)

    async def generate(self, *, system: str, prompt: str) -> str:
        response = await self.model.ainvoke(
            [
                ("system", system),
                ("user", prompt),
            ]
        )
        return _coerce_message_text(response)

    async def aclose(self) -> None:
        await _maybe_aclose(self.model)


class MemoryTopology:
    """Coordinates the shared store and one local store per role."""

    def __init__(
        self,
        *,
        roots: MemoryRoots,
        shared_engine: MemoryEngine,
        agent_engines: dict[str, MemoryEngine],
        resources: Optional[list[Any]] = None,
    ) -> None:
        self.roots = roots
        self.shared_engine = shared_engine
        self.agent_engines = agent_engines
        self.shared_writes: list[MemoryWriteRecord] = []
        self.agent_writes: list[MemoryWriteRecord] = []
        self._message_index = 0
        self._resources = resources or []

    @classmethod
    async def create(
        cls,
        *,
        roots: MemoryRoots,
        provider_factory: Callable[[str], Any],
        max_recall: int = 5,
        consolidation_session_gate: int = 2,
    ) -> "MemoryTopology":
        roots.ensure()
        seen_resources: set[int] = set()
        resources: list[Any] = []

        async def build_engine(scope: str, memory_dir: Path) -> MemoryEngine:
            provider = provider_factory(scope)
            if id(provider) not in seen_resources:
                seen_resources.add(id(provider))
                resources.append(provider)
            config = MemoryConfig(
                str(memory_dir),
                max_recall=max_recall,
                consolidation_session_gate=consolidation_session_gate,
            )
            return await MemoryEngine.create(config, provider)

        shared_engine = await build_engine("shared", roots.shared_dir)
        agent_engines = {
            role: await build_engine(role, roots.agent_dirs[role])
            for role in ROLE_ORDER
        }
        return cls(
            roots=roots,
            shared_engine=shared_engine,
            agent_engines=agent_engines,
            resources=resources,
        )

    def _next_message_uuid(self, label: str) -> str:
        self._message_index += 1
        return f"{label}-{self._message_index}"

    async def _upsert_memory(
        self,
        *,
        engine: MemoryEngine,
        scope: str,
        target: str,
        actor: str,
        name: str,
        description: str,
        memory_type: MemoryType,
        content: str,
    ) -> str:
        memory = Memory(name=name, description=description, memory_type=memory_type, content=content)
        filename = _slugify(name)
        try:
            engine.read_memory(filename)
        except RuntimeError:
            await engine.create_memory(memory)
        else:
            await engine.update_memory(filename, memory)

        record = MemoryWriteRecord(
            scope=scope,
            target=target,
            actor=actor,
            name=name,
            filename=filename,
            memory_type=_memory_type_name(memory_type),
        )
        if scope == "shared":
            self.shared_writes.append(record)
        else:
            self.agent_writes.append(record)
        return filename

    async def seed_defaults(
        self,
        *,
        task_brief: str,
        project_constraints: Sequence[str],
    ) -> None:
        await self._upsert_memory(
            engine=self.shared_engine,
            scope="shared",
            target="shared",
            actor="seed",
            name="workspace task brief",
            description="task brief for the multiagent software delivery evaluation",
            memory_type=MemoryType.Project,
            content=task_brief,
        )

        constraints = [constraint.strip() for constraint in project_constraints if constraint.strip()]
        if constraints:
            await self._upsert_memory(
                engine=self.shared_engine,
                scope="shared",
                target="shared",
                actor="seed",
                name="workspace constraints",
                description="shared project constraints for the multiagent software delivery evaluation",
                memory_type=MemoryType.Project,
                content="\n".join(f"- {constraint}" for constraint in constraints),
            )

        for role, spec in ROLE_SPECS.items():
            await self._upsert_memory(
                engine=self.agent_engines[role],
                scope="agent",
                target=role,
                actor="seed",
                name=spec.charter_name,
                description=spec.charter_description,
                memory_type=MemoryType.Reference,
                content=spec.charter_content,
            )

    async def recall_for_role(self, role: str, query: str) -> tuple[Any, Any, Any, Any]:
        agent_engine = self.agent_engines[role]
        local_manifest = agent_engine.manifest()
        shared_manifest = self.shared_engine.manifest()
        local_result = await agent_engine.recall(query, [])
        shared_result = await self.shared_engine.recall(query, [])
        return local_result, shared_result, local_manifest, shared_manifest

    async def extract_agent_turn(
        self,
        *,
        role: str,
        task_brief: str,
        project_constraints: Sequence[str],
        query: str,
        raw_output: str,
    ) -> None:
        constraints = [constraint.strip() for constraint in project_constraints if constraint.strip()]
        user_content = "\n\n".join(
            part
            for part in [
                f"Role: {role}",
                f"Task brief:\n{task_brief.strip()}",
                f"Recall query:\n{query.strip()}",
                "Constraints:\n" + "\n".join(f"- {constraint}" for constraint in constraints)
                if constraints
                else "",
            ]
            if part
        )
        messages = [
            Message(self._next_message_uuid(f"{role}-user"), MessageRole.User, user_content),
            Message(self._next_message_uuid(f"{role}-assistant"), MessageRole.Assistant, raw_output),
        ]
        await self.agent_engines[role].extract(messages)

    async def promote_shared_memories(
        self,
        candidates: Sequence[PromotableMemory],
        *,
        actor: str = "coordinator",
    ) -> list[str]:
        promoted: list[str] = []
        seen: set[str] = set()
        for candidate in candidates:
            if candidate.name in seen:
                continue
            seen.add(candidate.name)
            await self._upsert_memory(
                engine=self.shared_engine,
                scope="shared",
                target="shared",
                actor=actor,
                name=candidate.name,
                description=candidate.description,
                memory_type=_memory_type_enum(candidate.memory_type),
                content=candidate.content,
            )
            promoted.append(_slugify(candidate.name))
        return promoted

    async def apply_targeted_feedback(
        self,
        feedback_items: Sequence[TargetedFeedback],
        *,
        actor: str = "coordinator",
    ) -> list[str]:
        filenames: list[str] = []
        seen: set[tuple[str, str]] = set()
        for feedback in feedback_items:
            key = (feedback.target_role, feedback.name)
            if key in seen:
                continue
            seen.add(key)
            await self._upsert_memory(
                engine=self.agent_engines[feedback.target_role],
                scope="agent",
                target=feedback.target_role,
                actor=actor,
                name=feedback.name,
                description=feedback.description,
                memory_type=MemoryType.Feedback,
                content=feedback.content,
            )
            filenames.append(_slugify(feedback.name))
        return filenames

    def manifest_sizes(self) -> dict[str, ManifestSize]:
        manifests = {"shared": self.shared_engine.manifest()}
        manifests.update({role: engine.manifest() for role, engine in self.agent_engines.items()})
        return {
            scope: ManifestSize(
                entry_count=len(manifest.entries),
                line_count=manifest.line_count,
                byte_size=manifest.byte_size,
            )
            for scope, manifest in manifests.items()
        }

    async def record_session_end_all(self) -> None:
        await self.shared_engine.record_session_end()
        for engine in self.agent_engines.values():
            await engine.record_session_end()

    async def consolidate_background_all(self) -> None:
        await self.shared_engine.consolidate_background()
        for engine in self.agent_engines.values():
            await engine.consolidate_background()

    async def shutdown(self) -> None:
        await self.shared_engine.shutdown()
        for engine in self.agent_engines.values():
            await engine.shutdown()
        for resource in self._resources:
            await _maybe_aclose(resource)


def _build_recall_query(role: str, state: AgentGraphState) -> str:
    sections = [f"Task brief:\n{state['task_brief']}"]
    constraints = state["project_constraints"]
    if constraints:
        sections.append("Project constraints:\n" + "\n".join(f"- {constraint}" for constraint in constraints))

    outputs = state["outputs"]
    if role != "planner" and outputs.get("planner"):
        sections.append(f"Planner output:\n{outputs['planner']}")
    if role == "reviewer" and outputs.get("implementer"):
        sections.append(f"Implementer output:\n{outputs['implementer']}")

    return "\n\n".join(sections)


def _build_agent_prompt(
    *,
    spec: RoleSpec,
    state: AgentGraphState,
    local_memories: Sequence[Memory],
    shared_memories: Sequence[Memory],
) -> str:
    outputs = state["outputs"]
    prior_sections: list[str] = []
    if outputs.get("planner") and spec.role != "planner":
        prior_sections.append(f"Planner output:\n{outputs['planner']}")
    if outputs.get("implementer") and spec.role == "reviewer":
        prior_sections.append(f"Implementer output:\n{outputs['implementer']}")

    constraint_lines = state["project_constraints"]
    prompt_sections = [
        f"Role: {spec.title}",
        f"Focus: {spec.focus}",
        f"Task brief:\n{state['task_brief']}",
        "Project constraints:\n" + "\n".join(f"- {constraint}" for constraint in constraint_lines)
        if constraint_lines
        else "Project constraints:\n- None",
        "\n\n".join(prior_sections) if prior_sections else "Prior agent outputs:\n- None",
        f"Local memory recall:\n{_format_recalled_memories(local_memories)}",
        f"Shared memory recall:\n{_format_recalled_memories(shared_memories)}",
        (
            "Respond with ONLY valid JSON using this schema:\n"
            "{\n"
            '  "summary": "brief role output",\n'
            '  "promotable_memories": [\n'
            "    {\n"
            '      "name": "shared memory title",\n'
            '      "description": "why this should be recalled later",\n'
            '      "memory_type": "project",\n'
            '      "content": "markdown content"\n'
            "    }\n"
            "  ],\n"
            '  "targeted_feedback": [\n'
            "    {\n"
            '      "target_role": "implementer",\n'
            '      "name": "feedback title",\n'
            '      "description": "why the implementer should remember this",\n'
            '      "content": "markdown content"\n'
            "    }\n"
            "  ]\n"
            "}\n"
            "Use at most two promotable memories. Only the reviewer should normally emit targeted_feedback. "
            "Use an empty list when there is nothing worth storing."
        ),
    ]
    return "\n\n".join(prompt_sections)


def _collect_promotable_memories(parsed_outputs: Mapping[str, Mapping[str, Any]]) -> list[PromotableMemory]:
    collected: list[PromotableMemory] = []
    for payload in parsed_outputs.values():
        for item in payload.get("promotable_memories", []):
            if not isinstance(item, Mapping):
                continue
            content = str(item.get("content") or "").strip()
            if not content:
                continue
            collected.append(
                PromotableMemory(
                    name=str(item.get("name") or "shared memory").strip(),
                    description=str(item.get("description") or "shared project memory").strip(),
                    memory_type=_normalize_memory_type(str(item.get("memory_type") or "project")),
                    content=content,
                    source_role=str(item.get("source_role") or "unknown"),
                )
            )
    return collected


def _collect_targeted_feedback(parsed_outputs: Mapping[str, Mapping[str, Any]]) -> list[TargetedFeedback]:
    collected: list[TargetedFeedback] = []
    for payload in parsed_outputs.values():
        for item in payload.get("targeted_feedback", []):
            if not isinstance(item, Mapping):
                continue
            target_role = str(item.get("target_role") or "").strip().lower()
            content = str(item.get("content") or "").strip()
            if target_role not in ROLE_SPECS or not content:
                continue
            collected.append(
                TargetedFeedback(
                    target_role=target_role,
                    name=str(item.get("name") or f"{target_role} feedback").strip(),
                    description=str(item.get("description") or "targeted implementation feedback").strip(),
                    content=content,
                    source_role=str(item.get("source_role") or "unknown"),
                )
            )
    return collected


def _build_expectations(
    *,
    turn_records: Sequence[TurnRecord],
    shared_writes: Sequence[MemoryWriteRecord],
    agent_writes: Sequence[MemoryWriteRecord],
) -> list[ExpectationResult]:
    turn_index = {turn.role: turn for turn in turn_records}
    implementer_turn = turn_index.get("implementer")
    reviewer_turn = turn_index.get("reviewer")

    expectations = [
        ExpectationResult(
            name="all_roles_ran",
            passed=all(role in turn_index for role in ROLE_ORDER),
            detail="Planner, implementer, and reviewer each completed one turn.",
        ),
        ExpectationResult(
            name="planner_saw_seed_shared_memory",
            passed=bool(turn_index.get("planner") and turn_index["planner"].shared_recalled_filenames),
            detail="The planner recalled the seeded shared task context before the workflow began.",
        ),
        ExpectationResult(
            name="review_feedback_promoted_locally",
            passed=any(
                write.actor == "coordinator" and write.target == "implementer" and write.memory_type == "feedback"
                for write in agent_writes
            ),
            detail="Coordinator promoted reusable reviewer feedback into the implementer's local store.",
        ),
        ExpectationResult(
            name="shared_writes_owned_by_seed_or_coordinator",
            passed=all(write.actor in {"seed", "coordinator"} for write in shared_writes),
            detail="Shared-memory writes are restricted to seeding and coordinator promotion paths.",
        ),
        ExpectationResult(
            name="reviewer_completed_review",
            passed=bool(reviewer_turn and reviewer_turn.output_summary),
            detail="Reviewer produced a non-empty review output.",
        ),
    ]
    return expectations


def build_graph(topology: MemoryTopology, agent_model: AgentModel) -> Any:
    _require_langgraph()

    def build_role_node(role: str) -> Callable[[AgentGraphState], Any]:
        spec = ROLE_SPECS[role]

        async def run_role(state: AgentGraphState) -> dict[str, Any]:
            query = _build_recall_query(role, state)
            start = time.perf_counter()
            local_result, shared_result, local_manifest, shared_manifest = await topology.recall_for_role(role, query)
            prompt = _build_agent_prompt(
                spec=spec,
                state=state,
                local_memories=local_result.memories,
                shared_memories=shared_result.memories,
            )
            raw_output = await agent_model.generate(system=spec.system_prompt, prompt=prompt)
            parsed = parse_agent_response(raw_output, role)
            await topology.extract_agent_turn(
                role=role,
                task_brief=state["task_brief"],
                project_constraints=state["project_constraints"],
                query=query,
                raw_output=raw_output,
            )

            turn_record = TurnRecord(
                role=role,
                query=query,
                output_summary=parsed.summary,
                local_recalled_filenames=[_memory_filename(memory) for memory in local_result.memories],
                shared_recalled_filenames=[_memory_filename(memory) for memory in shared_result.memories],
                local_recall_miss=bool(local_manifest.entries) and not local_result.memories,
                shared_recall_miss=bool(shared_manifest.entries) and not shared_result.memories,
                latency_ms=int((time.perf_counter() - start) * 1000),
            )

            outputs = dict(state["outputs"])
            outputs[role] = parsed.summary
            parsed_outputs = dict(state["parsed_outputs"])
            parsed_outputs[role] = asdict(parsed)
            turn_records = list(state["turn_records"])
            turn_records.append(asdict(turn_record))
            return {
                "outputs": outputs,
                "parsed_outputs": parsed_outputs,
                "turn_records": turn_records,
            }

        return run_role

    async def coordinator_finalize(state: AgentGraphState) -> dict[str, Any]:
        promotable_memories = _collect_promotable_memories(state["parsed_outputs"])
        targeted_feedback = _collect_targeted_feedback(state["parsed_outputs"])
        promoted_shared = await topology.promote_shared_memories(promotable_memories, actor="coordinator")
        promoted_feedback = await topology.apply_targeted_feedback(targeted_feedback, actor="coordinator")
        return {
            "promoted_shared": list(state["promoted_shared"]) + promoted_shared,
            "targeted_feedback": list(state["targeted_feedback"]) + promoted_feedback,
        }

    graph = StateGraph(AgentGraphState)
    graph.add_node("planner", build_role_node("planner"))
    graph.add_node("implementer", build_role_node("implementer"))
    graph.add_node("reviewer", build_role_node("reviewer"))
    graph.add_node("coordinator_finalize", coordinator_finalize)
    graph.add_edge(START, "planner")
    graph.add_edge("planner", "implementer")
    graph.add_edge("implementer", "reviewer")
    graph.add_edge("reviewer", "coordinator_finalize")
    graph.add_edge("coordinator_finalize", END)
    return graph.compile()


def write_run_summary(summary: PocRunSummary, destination: Path) -> Path:
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("w", encoding="utf-8") as handle:
        json.dump(summary.to_dict(), handle, indent=2, sort_keys=True)
        handle.write("\n")
    summary.summary_path = str(destination)
    return destination


def format_console_summary(summary: PocRunSummary) -> str:
    lines = [
        f"Run ID: {summary.run_id}",
        f"Model: {summary.model}",
        f"Run directory: {summary.run_dir}",
        f"Summary JSON: {summary.summary_path or 'not written'}",
        f"Total latency: {summary.total_latency_ms} ms",
        "",
        "Turns:",
    ]
    for turn in summary.turn_records:
        lines.append(
            f"- {turn.role}: {turn.latency_ms} ms | "
            f"local={turn.local_recalled_filenames or ['<none>']} | "
            f"shared={turn.shared_recalled_filenames or ['<none>']}"
        )
    lines.extend(
        [
            "",
            f"Promoted shared memories: {summary.promoted_shared or ['<none>']}",
            f"Targeted feedback memories: {summary.targeted_feedback or ['<none>']}",
            "",
            "Manifest sizes:",
        ]
    )
    for scope, manifest in summary.manifest_sizes.items():
        lines.append(
            f"- {scope}: entries={manifest.entry_count}, lines={manifest.line_count}, bytes={manifest.byte_size}"
        )
    lines.extend(["", "Expectations:"])
    for expectation in summary.expectations:
        status = "PASS" if expectation.passed else "FAIL"
        lines.append(f"- {status} {expectation.name}: {expectation.detail}")
    return "\n".join(lines)


async def run_langgraph_poc(
    *,
    task_brief: str,
    project_constraints: Optional[Sequence[str]] = None,
    base_memory_dir: Optional[Union[os.PathLike[str], str]] = None,
    run_id: Optional[str] = None,
    model: Optional[str] = None,
    memory_provider_factory: Optional[Callable[[str], Any]] = None,
    agent_model: Optional[AgentModel] = None,
    max_recall: int = 5,
    run_consolidation: bool = False,
) -> PocRunSummary:
    _require_langgraph()

    constraints = [constraint.strip() for constraint in (project_constraints or []) if constraint.strip()]
    resolved_model = model or os.environ.get("OPENAI_MODEL")
    if agent_model is None and not resolved_model:
        raise ValueError("OPENAI_MODEL or --model is required when no custom agent_model is supplied.")
    if memory_provider_factory is None and not resolved_model:
        raise ValueError("OPENAI_MODEL or --model is required when no custom memory provider is supplied.")

    if (agent_model is None or memory_provider_factory is None) and not os.environ.get("OPENAI_API_KEY"):
        raise ValueError("OPENAI_API_KEY is required when using the default OpenAI-backed providers.")

    run_id = run_id or os.environ.get("BLACKSWAN_POC_RUN_ID") or f"run-{uuid.uuid4().hex[:8]}"
    base_dir = Path(base_memory_dir or os.environ.get("BLACKSWAN_POC_MEMORY_DIR", ".blackswan-poc")).expanduser()
    roots = MemoryRoots.from_base(base_dir.resolve(), run_id)

    if memory_provider_factory is None:
        def memory_provider_factory(_scope: str) -> OpenAIMemoryProvider:
            return OpenAIMemoryProvider(model=resolved_model or "")

    if agent_model is None:
        agent_model = OpenAIAgentModel(model=resolved_model or "")

    topology = await MemoryTopology.create(
        roots=roots,
        provider_factory=memory_provider_factory,
        max_recall=max_recall,
    )

    start_time = _utc_now()
    start_perf = time.perf_counter()
    summary_path = roots.run_dir / "summary.json"
    try:
        await topology.seed_defaults(task_brief=task_brief, project_constraints=constraints)
        graph = build_graph(topology, agent_model)
        final_state = await graph.ainvoke(
            {
                "run_id": run_id,
                "task_brief": task_brief,
                "project_constraints": list(constraints),
                "outputs": {},
                "parsed_outputs": {},
                "turn_records": [],
                "promoted_shared": [],
                "targeted_feedback": [],
            }
        )

        end_time = _utc_now()
        turn_records = [
            record if isinstance(record, TurnRecord) else TurnRecord(**record)
            for record in final_state["turn_records"]
        ]
        manifest_sizes = topology.manifest_sizes()
        expectations = _build_expectations(
            turn_records=turn_records,
            shared_writes=topology.shared_writes,
            agent_writes=topology.agent_writes,
        )
        summary = PocRunSummary(
            run_id=run_id,
            model=resolved_model or "<custom>",
            run_dir=str(roots.run_dir),
            task_brief=task_brief,
            project_constraints=list(constraints),
            started_at=start_time.isoformat(),
            finished_at=end_time.isoformat(),
            total_latency_ms=int((time.perf_counter() - start_perf) * 1000),
            turn_records=turn_records,
            promoted_shared=list(final_state["promoted_shared"]),
            targeted_feedback=list(final_state["targeted_feedback"]),
            manifest_sizes=manifest_sizes,
            expectations=expectations,
            shared_writes=list(topology.shared_writes),
            agent_writes=list(topology.agent_writes),
            outputs=dict(final_state["outputs"]),
        )
        write_run_summary(summary, summary_path)
        return summary
    finally:
        await topology.record_session_end_all()
        if run_consolidation:
            await topology.consolidate_background_all()
        await topology.shutdown()
        await _maybe_aclose(agent_model)


__all__ = [
    "AgentModel",
    "MemoryRoots",
    "MemoryTopology",
    "OpenAIAgentModel",
    "OpenAIMemoryProvider",
    "ParsedAgentResponse",
    "PocRunSummary",
    "ROLE_ORDER",
    "ROLE_SPECS",
    "TargetedFeedback",
    "build_graph",
    "format_console_summary",
    "parse_agent_response",
    "run_langgraph_poc",
    "write_run_summary",
]
