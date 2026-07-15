export type DocumentPersistenceStatus =
  | "clean"
  | "dirty"
  | "saving"
  | "saved"
  | "saved_index_degraded"
  | "failed";

export interface DocumentPersistenceWriteResult {
  indexDegraded: boolean;
}

export interface DocumentPersistenceSnapshot {
  path: string;
  markdown: string;
  revision: number;
  baselineMarkdown: string;
  baselineRevision: number;
  savedAt: number | null;
  indexDegraded: boolean;
  status: DocumentPersistenceStatus;
  error: string | null;
}

interface DocumentRecord extends DocumentPersistenceSnapshot {
  discarded: boolean;
  timer: ReturnType<typeof setTimeout> | null;
  writeTask: Promise<void> | null;
}

interface DocumentPersistenceCoordinatorOptions {
  delayMs?: number;
  write: (
    path: string,
    markdown: string,
  ) => Promise<DocumentPersistenceWriteResult>;
}

type DocumentPersistenceListener = (
  snapshot: DocumentPersistenceSnapshot,
) => void;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Serializes Markdown persistence per document path and prevents stale write
 * receipts from acknowledging a newer captured revision.
 */
export class DocumentPersistenceCoordinator {
  private readonly delayMs: number;
  private readonly records = new Map<string, DocumentRecord>();
  private readonly listeners = new Set<DocumentPersistenceListener>();
  private readonly write: DocumentPersistenceCoordinatorOptions["write"];
  private revision = 0;

  constructor({
    delayMs = 1200,
    write,
  }: DocumentPersistenceCoordinatorOptions) {
    this.delayMs = delayMs;
    this.write = write;
  }

  /** Establishes an authoritative on-disk baseline for a document. */
  load(path: string, markdown: string): DocumentPersistenceSnapshot {
    this.discard(path);
    const revision = this.nextRevision();
    const record: DocumentRecord = {
      path,
      markdown,
      revision,
      baselineMarkdown: markdown,
      baselineRevision: revision,
      savedAt: Date.now(),
      indexDegraded: false,
      status: "clean",
      error: null,
      discarded: false,
      timer: null,
      writeTask: null,
    };
    this.records.set(path, record);
    this.emit(record);
    return this.snapshot(record);
  }

  /** Captures a complete Markdown snapshot and schedules its delayed commit. */
  capture(path: string, markdown: string): DocumentPersistenceSnapshot {
    if (!this.records.has(path)) {
      const revision = this.nextRevision();
      const record: DocumentRecord = {
        path,
        markdown,
        revision,
        baselineMarkdown: "",
        baselineRevision: 0,
        savedAt: null,
        indexDegraded: false,
        status: "dirty",
        error: null,
        discarded: false,
        timer: null,
        writeTask: null,
      };
      this.records.set(path, record);
      this.schedule(record);
      this.emit(record);
      return this.snapshot(record);
    }
    const record = this.requireRecord(path);
    record.markdown = markdown;
    record.revision = this.nextRevision();
    record.error = null;
    if (record.baselineMarkdown === markdown && !record.writeTask) {
      record.baselineRevision = record.revision;
      record.status = "clean";
    } else {
      record.status = "dirty";
      this.schedule(record);
    }
    this.emit(record);
    return this.snapshot(record);
  }

  /** Commits the currently captured revision once, without waiting for later edits. */
  async commit(path: string): Promise<DocumentPersistenceSnapshot> {
    const record = this.requireRecord(path);
    this.cancelTimer(record);
    if (record.baselineRevision === record.revision) {
      return this.snapshot(record);
    }
    if (record.writeTask) {
      await record.writeTask;
      if (
        this.records.get(record.path) === record &&
        record.baselineRevision !== record.revision
      ) {
        this.schedule(record);
      }
      return this.snapshot(record);
    }
    await this.writeCurrent(record);
    return this.snapshot(record);
  }

  /** Waits until every captured revision for this path is durably acknowledged. */
  async barrier(path: string): Promise<DocumentPersistenceSnapshot> {
    let record = this.requireRecord(path);
    this.cancelTimer(record);
    while (record.baselineRevision !== record.revision) {
      await this.commit(record.path);
      record = this.requireRecord(path);
    }
    return this.snapshot(record);
  }

  /** Flushes the old path before moving it, then moves later captures to the new path. */
  async rename(
    oldPath: string,
    newPath: string,
    move: () => Promise<string>,
  ): Promise<DocumentPersistenceSnapshot> {
    await this.barrier(oldPath);
    const reboundPath = await move();
    return this.rebind(oldPath, reboundPath || newPath);
  }

  /** Rebinds a known document snapshot after an external path move. */
  rebind(oldPath: string, newPath: string): DocumentPersistenceSnapshot {
    const source = this.requireRecord(oldPath);
    const destination = this.records.get(newPath);
    if (destination && destination !== source) {
      if (destination.revision > source.revision) {
        this.discard(oldPath);
        return this.snapshot(destination);
      }
      this.discard(newPath);
    }
    this.records.delete(oldPath);
    source.path = newPath;
    this.records.set(newPath, source);
    if (source.baselineRevision !== source.revision) {
      source.status = "dirty";
      this.schedule(source);
    }
    this.emit(source);
    return this.snapshot(source);
  }

  /** Stops pending work and forgets a document that was deleted or discarded. */
  discard(path: string): Promise<void> {
    const record = this.records.get(path);
    if (!record) return Promise.resolve();
    record.discarded = true;
    this.cancelTimer(record);
    this.records.delete(path);
    return record.writeTask ?? Promise.resolve();
  }

  /** Returns the visible persistence state for a path. */
  get(path: string): DocumentPersistenceSnapshot | null {
    const record = this.records.get(path);
    return record ? this.snapshot(record) : null;
  }

  subscribe(listener: DocumentPersistenceListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private nextRevision(): number {
    this.revision += 1;
    return this.revision;
  }

  private requireRecord(path: string): DocumentRecord {
    const record = this.records.get(path);
    if (!record) {
      throw new Error(`no recoverable snapshot for ${path}`);
    }
    return record;
  }

  private schedule(record: DocumentRecord): void {
    this.cancelTimer(record);
    record.timer = setTimeout(() => {
      record.timer = null;
      void this.commit(record.path).catch(() => undefined);
    }, this.delayMs);
  }

  private cancelTimer(record: DocumentRecord): void {
    if (!record.timer) return;
    clearTimeout(record.timer);
    record.timer = null;
  }

  private async writeCurrent(record: DocumentRecord): Promise<void> {
    const path = record.path;
    const markdown = record.markdown;
    const revision = record.revision;
    record.status = "saving";
    this.emit(record);
    const task = (async () => {
      try {
        const result = await this.write(path, markdown);
        if (
          record.discarded ||
          this.records.get(record.path) !== record ||
          record.path !== path
        ) {
          return;
        }
        if (record.revision !== revision) {
          record.status = "dirty";
          this.emit(record);
          return;
        }
        record.baselineMarkdown = markdown;
        record.baselineRevision = revision;
        record.savedAt = Date.now();
        record.indexDegraded = result.indexDegraded;
        record.status = result.indexDegraded ? "saved_index_degraded" : "saved";
        record.error = null;
        this.emit(record);
      } catch (error) {
        if (!record.discarded && this.records.get(record.path) === record) {
          record.status = "failed";
          record.error = errorMessage(error);
          this.emit(record);
        }
        throw error;
      } finally {
        record.writeTask = null;
      }
    })();
    record.writeTask = task;
    await task;
  }

  private emit(record: DocumentRecord): void {
    const snapshot = this.snapshot(record);
    for (const listener of this.listeners) {
      listener(snapshot);
    }
  }

  private snapshot(record: DocumentRecord): DocumentPersistenceSnapshot {
    const {
      baselineMarkdown,
      baselineRevision,
      error,
      indexDegraded,
      markdown,
      path,
      revision,
      savedAt,
      status,
    } = record;
    return {
      path,
      markdown,
      revision,
      baselineMarkdown,
      baselineRevision,
      savedAt,
      indexDegraded,
      status,
      error,
    };
  }
}
