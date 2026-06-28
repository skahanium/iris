export type DocumentOpenPriority = "foreground" | "hot" | "warm" | "background";

export type NoteOpenSource =
  | "welcome"
  | "quick-open"
  | "file-tree"
  | "tab"
  | "link"
  | "search"
  | "graph"
  | "outline"
  | "ai"
  | "management"
  | "recycle"
  | "classified"
  | "startup"
  | "test";

export interface DocumentOpenJob<T> {
  key: string;
  namespace: string;
  path: string;
  source: NoteOpenSource;
  priority: DocumentOpenPriority;
  run: (signal: AbortSignal) => Promise<T>;
}

export interface EnqueuedDocumentOpen<T> {
  promise: Promise<T>;
  cancel: () => void;
}

interface QueueEntry<T> {
  activeHandleId: number;
  sequence: number;
  job: DocumentOpenJob<T>;
  controller: AbortController;
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
}

const PRIORITY_RANK: Record<DocumentOpenPriority, number> = {
  foreground: 0,
  hot: 1,
  warm: 2,
  background: 3,
};

function abortError(): Error {
  const error = new Error("Document open job cancelled");
  error.name = "AbortError";
  return error;
}

function outranks(
  next: DocumentOpenPriority,
  current: DocumentOpenPriority,
): boolean {
  return PRIORITY_RANK[next] < PRIORITY_RANK[current];
}

function canBypassConcurrency(priority: DocumentOpenPriority): boolean {
  return priority === "foreground" || priority === "hot";
}

export class DocumentOpenScheduler {
  private readonly maxConcurrent: number;
  private nextSequence = 0;
  private pumpQueued = false;
  private nextHandleId = 0;
  private readonly pending: QueueEntry<unknown>[] = [];
  private readonly queuedByKey = new Map<string, QueueEntry<unknown>>();
  private readonly runningByKey = new Map<string, Promise<unknown>>();

  constructor(options: { maxConcurrent?: number } = {}) {
    this.maxConcurrent = Math.max(1, options.maxConcurrent ?? 2);
  }

  enqueue<T>(job: DocumentOpenJob<T>): EnqueuedDocumentOpen<T> {
    const running = this.runningByKey.get(job.key);
    if (running) {
      return {
        promise: running as Promise<T>,
        cancel: () => undefined,
      };
    }

    const queued = this.queuedByKey.get(job.key);
    if (queued) {
      const handleId = this.nextHandleId;
      this.nextHandleId += 1;
      if (outranks(job.priority, queued.job.priority)) {
        queued.job = {
          ...queued.job,
          priority: job.priority,
          source: job.source,
        };
        queued.activeHandleId = handleId;
        this.sortPending();
      }
      return {
        promise: queued.promise as Promise<T>,
        cancel: () => this.cancelEntry(queued, handleId),
      };
    }

    const controller = new AbortController();
    let resolve!: (value: T) => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<T>((innerResolve, innerReject) => {
      resolve = innerResolve;
      reject = innerReject;
    });

    const handleId = this.nextHandleId;
    this.nextHandleId += 1;

    const entry: QueueEntry<T> = {
      activeHandleId: handleId,
      sequence: this.nextSequence,
      job,
      controller,
      promise,
      resolve,
      reject,
    };
    this.nextSequence += 1;

    this.pending.push(entry as QueueEntry<unknown>);
    this.queuedByKey.set(job.key, entry as QueueEntry<unknown>);
    this.sortPending();
    this.queuePump();

    return {
      promise,
      cancel: () => this.cancelEntry(entry as QueueEntry<unknown>, handleId),
    };
  }

  promote(
    key: string,
    priority: DocumentOpenPriority,
    source: NoteOpenSource,
  ): void {
    const queued = this.queuedByKey.get(key);
    if (!queued || !outranks(priority, queued.job.priority)) {
      return;
    }
    queued.job = { ...queued.job, priority, source };
    queued.activeHandleId = this.nextHandleId;
    this.nextHandleId += 1;
    this.sortPending();
  }
  private cancelEntry(entry: QueueEntry<unknown>, handleId: number): void {
    if (!this.queuedByKey.has(entry.job.key)) {
      return;
    }
    if (entry.activeHandleId !== handleId) {
      return;
    }
    entry.controller.abort();
    this.queuedByKey.delete(entry.job.key);
    const index = this.pending.indexOf(entry);
    if (index >= 0) {
      this.pending.splice(index, 1);
    }
    entry.reject(abortError());
  }

  private queuePump(): void {
    if (this.pumpQueued) {
      return;
    }
    this.pumpQueued = true;
    queueMicrotask(() => {
      this.pumpQueued = false;
      this.pump();
    });
  }

  private pump(): void {
    while (this.pending.length > 0) {
      const underLimit = this.runningByKey.size < this.maxConcurrent;
      const bypassingLimit = !underLimit;
      const nextIndex = underLimit
        ? 0
        : this.pending.findIndex((entry) =>
            canBypassConcurrency(entry.job.priority),
          );
      if (nextIndex < 0) {
        return;
      }
      const entry = this.pending.splice(nextIndex, 1)[0];
      if (!entry) {
        return;
      }
      this.queuedByKey.delete(entry.job.key);
      if (entry.controller.signal.aborted) {
        entry.reject(abortError());
        continue;
      }
      const running = entry.job
        .run(entry.controller.signal)
        .then((value) => {
          entry.resolve(value);
          return value;
        })
        .catch((error) => {
          entry.reject(error);
          throw error;
        })
        .finally(() => {
          this.runningByKey.delete(entry.job.key);
          this.queuePump();
        });
      this.runningByKey.set(entry.job.key, running);
      void running.catch(() => undefined);
      if (bypassingLimit) {
        return;
      }
    }
  }

  private sortPending(): void {
    this.pending.sort((a, b) => {
      const priority =
        PRIORITY_RANK[a.job.priority] - PRIORITY_RANK[b.job.priority];
      return priority === 0 ? a.sequence - b.sequence : priority;
    });
  }
}
