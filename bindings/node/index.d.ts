export type MemoryType = 'User' | 'Feedback' | 'Project' | 'Reference'
export type MessageRole = 'User' | 'Assistant' | 'System'

export interface Message {
  uuid: string
  role: MessageRole
  content: string
}

export interface Memory {
  name: string
  description: string
  memoryType: MemoryType
  content: string
  path: string
  modified: number | null
}

export interface ManifestEntry {
  title: string
  filename: string
  hook: string
}

export interface MemoryManifest {
  entries: ManifestEntry[]
  lineCount: number
  byteSize: number
}

export interface RecallResult {
  memories: Memory[]
  filtered: string[]
}

export interface MemoryConfigOptions {
  memoryDir: string
  maxIndexLines?: number
  maxIndexBytes?: number
  maxScanFiles?: number
  maxRecall?: number
  extractionTurnInterval?: number
  consolidationSessionGate?: number
  enabled?: boolean
  bareMode?: boolean
  remoteMode?: boolean
}

export type LlmProvider = (messages: string, system: string | null) => Promise<string>

export class MemoryEngine {
  static create(config: MemoryConfigOptions, provider: LlmProvider): Promise<MemoryEngine>
  recall(query: string, recentlyUsedTools?: string[]): Promise<RecallResult>
  extract(messages: Message[]): Promise<void>
  extractBackground(messages: Message[]): void
  createMemory(memory: Memory): Promise<void>
  updateMemory(name: string, memory: Memory): Promise<void>
  deleteMemory(name: string): Promise<void>
  manifest(): MemoryManifest
  readMemory(name: string): Memory
  isEnabled(): boolean
  recordSessionEnd(): Promise<void>
  consolidate(): Promise<boolean>
  consolidateBackground(): Promise<void>
  shutdown(): Promise<void>
}
