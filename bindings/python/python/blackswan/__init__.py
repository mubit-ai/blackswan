"""blackswan - Persistent memory system for AI agents."""

from blackswan._native import (
    PyMemoryConfig as MemoryConfig,
    PyMemoryType as MemoryType,
    PyMemory as Memory,
    PyMessage as Message,
    PyMessageRole as MessageRole,
    PyRecallResult as RecallResult,
    PyManifestEntry as ManifestEntry,
    PyMemoryManifest as MemoryManifest,
    PyMemoryEngine as MemoryEngine,
)

__all__ = [
    "MemoryConfig",
    "MemoryType",
    "Memory",
    "Message",
    "MessageRole",
    "RecallResult",
    "ManifestEntry",
    "MemoryManifest",
    "MemoryEngine",
]
