export interface AssistantContentSummary {
  empty: boolean;
  hash: string;
  length: number;
}

export interface AssistantRenderWindow {
  content: string;
  fullHash: string;
  fullLength: number;
  omittedChars: number;
  truncated: boolean;
}

function updateFnv1a(hash: number, value: string): number {
  let next = hash;
  for (let index = 0; index < value.length; index += 1) {
    next ^= value.charCodeAt(index);
    next = Math.imul(next, 0x01000193) >>> 0;
  }
  return next;
}

export function assistantContentHash(value: string): string {
  return `h${updateFnv1a(0x811c9dc5, value).toString(16).padStart(8, "0")}`;
}

export class AssistantStreamBuffer {
  private chunks: string[] = [];
  private hashValue = 0x811c9dc5;
  private textLength = 0;

  get length(): number {
    return this.textLength;
  }

  append(token: string): void {
    if (!token) return;
    this.chunks.push(token);
    this.textLength += token.length;
    this.hashValue = updateFnv1a(this.hashValue, token);
  }

  clear(): void {
    this.chunks = [];
    this.hashValue = 0x811c9dc5;
    this.textLength = 0;
  }

  replace(value: string): void {
    this.clear();
    this.append(value);
  }

  summary(): AssistantContentSummary {
    return {
      empty: this.textLength === 0,
      hash: `h${this.hashValue.toString(16).padStart(8, "0")}`,
      length: this.textLength,
    };
  }

  renderWindow(maxChars: number): AssistantRenderWindow {
    const fullHash = this.summary().hash;
    const budget = Math.max(0, Math.floor(maxChars));
    if (this.textLength <= budget) {
      return {
        content: this.toString(),
        fullHash,
        fullLength: this.textLength,
        omittedChars: 0,
        truncated: false,
      };
    }

    if (budget === 0) {
      return {
        content: "",
        fullHash,
        fullLength: this.textLength,
        omittedChars: this.textLength,
        truncated: true,
      };
    }

    let remaining = budget;
    const parts: string[] = [];
    for (
      let index = this.chunks.length - 1;
      index >= 0 && remaining > 0;
      index -= 1
    ) {
      const chunk = this.chunks[index] ?? "";
      if (chunk.length <= remaining) {
        parts.push(chunk);
        remaining -= chunk.length;
      } else {
        parts.push(chunk.slice(chunk.length - remaining));
        remaining = 0;
      }
    }

    const content = parts.reverse().join("");
    return {
      content,
      fullHash,
      fullLength: this.textLength,
      omittedChars: this.textLength - content.length,
      truncated: true,
    };
  }

  toString(): string {
    return this.chunks.join("");
  }
}
