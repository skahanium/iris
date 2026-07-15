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
  migration: PathMigration | null;
  timer: ReturnType<typeof setTimeout> | null;
  writeTask: Promise<void> | null;
}

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
}

interface PathMigration {
  oldPath: string;
  record: DocumentRecord;
  ready: Deferred<void>;
}

interface DocumentPersistenceCoordinatorOptions {
  delayMs?: number;
  write: (
    path: string,
    markdown: string,
  ) => Promise<DocumentPersistenceWriteResult>;
}

type DocumentPersistenceListener = (
  snapshot: DocumentPersistenceSnapshot | null,
) => void;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

/**
 * Serializes Markdown persistence per document path and prevents stale write
 * receipts from acknowledging a newer captured revision.
 */
export class DocumentPersistenceCoordinator {
  private readonly delayMs: number;
  private readonly records = new Map<string, DocumentRecord>();
  private readonly pathRedirects = new Map<string, string>();
  private readonly migrations = new Map<string, PathMigration>();
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
      migration: null,
      timer: null,
      writeTask: null,
    };
    this.records.set(path, record);
    this.emit(record);
    return this.snapshot(record);
  }

  /** Captures a complete Markdown snapshot and schedules its delayed commit. */
  capture(path: string, markdown: string): DocumentPersistenceSnapshot {
    const resolvedPath = this.resolvePath(path);
    if (!this.records.has(resolvedPath)) {
      const revision = this.nextRevision();
      const record: DocumentRecord = {
        path: resolvedPath,
        markdown,
        revision,
        baselineMarkdown: "",
        baselineRevision: 0,
        savedAt: null,
        indexDegraded: false,
        status: "dirty",
        error: null,
        discarded: false,
        migration: null,
        timer: null,
        writeTask: null,
      };
      this.records.set(resolvedPath, record);
      this.schedule(record);
      this.emit(record);
      return this.snapshot(record);
    }
    const record = this.requireRecord(resolvedPath);
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
    const record = this.requireRecord(this.resolvePath(path));
    this.cancelTimer(record);
    if (record.migration) {
      await record.migration.ready.promise;
      return this.commit(record.path);
    }
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
    let record = this.requireRecord(this.resolvePath(path));
    this.cancelTimer(record);
    while (record.baselineRevision !== record.revision) {
      await this.commit(record.path);
      record = this.requireRecord(this.resolvePath(record.path));
    }
    return this.snapshot(record);
  }

  /**
   * Waits for a stable point where every tracked document revision has a
   * durable acknowledgement. Captures observed while an earlier pass writes
   * are included by the following pass rather than being missed by a caller's
   * one-time tab enumeration.
   */
  async barrierAll(): Promise<void> {
    while (true) {
      const observedRevision = this.revision;
      const paths = [...this.records.keys()];
      await Promise.all(paths.map((path) => this.barrier(path)));
      if (observedRevision === this.revision && !this.hasDirtyDocuments()) {
        return;
      }
    }
  }

  /** Flushes the old path before moving it, then moves later captures to the new path. */
  async rename(
    oldPath: string,
    newPath: string,
    move: () => Promise<string>,
  ): Promise<DocumentPersistenceSnapshot> {
    await this.beginPathMigration(oldPath, newPath);
    try {
      const reboundPath = await move();
      return this.completePathMigration(oldPath, reboundPath || newPath);
    } catch (error) {
      this.abortPathMigration(oldPath);
      throw error;
    }
  }

  /** Begins a path migration and prevents later captures from writing the old path. */
  async beginPathMigration(
    oldPath: string,
    newPath: string,
  ): Promise<DocumentPersistenceSnapshot> {
    const existing = this.migrations.get(oldPath);
    if (existing) return this.snapshot(existing.record);

    await this.barrier(oldPath);
    const source = this.requireRecord(oldPath);
    const destination = this.records.get(newPath);
    if (destination && destination !== source) {
      throw new Error(
        `path migration destination is already tracked: ${newPath}`,
      );
    }

    const migration: PathMigration = {
      oldPath,
      record: source,
      ready: deferred<void>(),
    };
    this.records.delete(oldPath);
    source.path = newPath;
    source.migration = migration;
    this.records.set(newPath, source);
    this.pathRedirects.set(oldPath, newPath);
    this.migrations.set(oldPath, migration);
    this.emit(source);
    return this.snapshot(source);
  }

  /** Completes a prepared migration after the external move has succeeded. */
  completePathMigration(
    oldPath: string,
    newPath: string,
  ): DocumentPersistenceSnapshot {
    const migration = this.migrations.get(oldPath);
    if (!migration) return this.rebind(oldPath, newPath);

    const source = migration.record;
    this.migrations.delete(oldPath);
    this.pathRedirects.delete(oldPath);
    this.rebindRecord(source, newPath);
    source.migration = null;
    migration.ready.resolve();
    if (source.baselineRevision !== source.revision) {
      source.status = "dirty";
      this.schedule(source);
    }
    this.emit(source);
    return this.snapshot(source);
  }

  /** Restores the old path when an external move fails. */
  abortPathMigration(oldPath: string): DocumentPersistenceSnapshot | null {
    const migration = this.migrations.get(oldPath);
    if (!migration) return null;

    const source = migration.record;
    this.migrations.delete(oldPath);
    this.pathRedirects.delete(oldPath);
    this.rebindRecord(source, oldPath);
    source.migration = null;
    migration.ready.resolve();
    if (source.baselineRevision !== source.revision) {
      source.status = "dirty";
      this.schedule(source);
    }
    this.emit(source);
    return this.snapshot(source);
  }

  /** Rebinds a known document snapshot after an external path move. */
  rebind(oldPath: string, newPath: string): DocumentPersistenceSnapshot {
    const migration = this.migrations.get(oldPath);
    if (migration) return this.completePathMigration(oldPath, newPath);
    const source = this.requireRecord(this.resolvePath(oldPath));
    this.rebindRecord(source, newPath);
    this.emit(source);
    return this.snapshot(source);
  }

  private rebindRecord(source: DocumentRecord, newPath: string): void {
    const oldPath = source.path;
    const destination = this.records.get(newPath);
    if (destination && destination !== source) {
      if (destination.revision > source.revision) {
        throw new Error(`path rebind destination is newer: ${newPath}`);
      }
      this.discard(newPath);
    }
    this.records.delete(oldPath);
    source.path = newPath;
    this.records.set(newPath, source);
  }

  /** Stops pending work and forgets a document that was deleted or discarded. */
  discard(path: string): Promise<void> {
    const resolvedPath = this.resolvePath(path);
    const record = this.records.get(resolvedPath);
    if (!record) return Promise.resolve();
    for (const [oldPath, migration] of this.migrations) {
      if (migration.record !== record) continue;
      this.migrations.delete(oldPath);
      this.pathRedirects.delete(oldPath);
      migration.ready.resolve();
    }
    record.discarded = true;
    this.cancelTimer(record);
    this.records.delete(resolvedPath);
    this.emit(null);
    return record.writeTask ?? Promise.resolve();
  }

  /** Returns the visible persistence state for a path. */
  get(path: string): DocumentPersistenceSnapshot | null {
    const record = this.records.get(this.resolvePath(path));
    return record ? this.snapshot(record) : null;
  }

  /** Reports whether a captured revision still lacks a disk acknowledgement. */
  hasDirtyDocuments(): boolean {
    return [...this.records.values()].some(
      (record) =>
        !record.discarded && record.baselineRevision !== record.revision,
    );
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

  private resolvePath(path: string): string {
    let resolvedPath = path;
    const visited = new Set<string>();
    while (this.pathRedirects.has(resolvedPath) && !visited.has(resolvedPath)) {
      visited.add(resolvedPath);
      resolvedPath = this.pathRedirects.get(resolvedPath)!;
    }
    return resolvedPath;
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

  private emit(record: DocumentRecord | null): void {
    const snapshot = record ? this.snapshot(record) : null;
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
