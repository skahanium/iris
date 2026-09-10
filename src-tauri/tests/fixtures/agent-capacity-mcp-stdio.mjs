#!/usr/bin/env node
// Deterministic MCP stdio peer used only by the Rust contract tests. It relies
// on Node built-ins because Iris launches stdio MCP peers with a cleared
// environment; the host must pass an absolute node executable.

import { writeSync } from "node:fs";
import { createInterface } from "node:readline";

const mode = process.argv[2] ?? "search-only";
const resultCount = Number.parseInt(process.argv[3] ?? "1", 10) || 1;
const fixtureTimestamp = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
const fixtureDate = fixtureTimestamp.slice(0, 10);

function writeLine(value) {
  writeSync(1, `${value}\n`);
}

function writeResponse(id, result) {
  writeLine(JSON.stringify({ jsonrpc: "2.0", id, result }));
}

function claimText() {
  let claims = "";
  for (let ordinal = 1; ordinal <= 48; ordinal += 1) {
    claims += ` fact-web-${ordinal}=value-${ordinal}`;
  }
  return `${claims} date: ${fixtureTimestamp}`;
}

function searchText() {
  const claims = claimText();
  if (resultCount > 1) {
    let results = `[1] title: Contract\nurl: https://source.invalid/contract\nsnippet: deterministic${claims}\n`;
    for (let index = 2; index <= resultCount; index += 1) {
      results += `[${index}] title: Result ${index}\nurl: https://source-${index}.invalid/${index}\nsnippet: deterministic${claims}\n`;
    }
    return results;
  }
  return `[1] title: Contract\nurl: https://source.invalid/contract\nsnippet: deterministic${claims}`;
}

if (mode === "malformed") {
  writeLine("not-json");
  process.exit(0);
}

const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of rl) {
  if (mode === "timeout") {
    continue;
  }
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    continue;
  }
  const id = message.id;
  const method = message.method;
  if (method === "initialize") {
    writeResponse(id, {
      protocolVersion: "2025-06-18",
      capabilities: { tools: {} },
      serverInfo: { name: "iris-contract-mcp", version: "1" },
    });
    continue;
  }
  if (method === "tools/list") {
    const tools = [
      {
        name: "search",
        annotations: { readOnlyHint: true },
        inputSchema: {
          type: "object",
          properties: {
            query: { type: "string" },
            max_results: { type: "integer" },
          },
          required: ["query"],
          additionalProperties: false,
        },
      },
    ];
    if (mode === "domain-dto") {
      tools.push({
        name: "domain",
        annotations: { readOnlyHint: true },
        inputSchema: {
          type: "object",
          properties: {},
          additionalProperties: false,
        },
      });
    } else if (mode === "search-fetch" || mode === "fetch-rate-limit") {
      tools.push({
        name: "fetch",
        annotations: { readOnlyHint: true },
        inputSchema: {
          type: "object",
          properties: { url: { type: "string" } },
          required: ["url"],
          additionalProperties: false,
        },
      });
    }
    writeResponse(id, { tools });
    continue;
  }
  if (method !== "tools/call") {
    continue;
  }
  const toolName = message.params?.name;
  if (toolName === "fetch") {
    if (mode === "fetch-rate-limit") {
      writeResponse(id, {
        content: [{ type: "text", text: '{"error":"Extract failed","status":429}' }],
        isError: false,
      });
      continue;
    }
    const requestedUrl = String(message.params?.arguments?.url ?? "");
    writeResponse(id, {
      content: [
        {
          type: "text",
          text: JSON.stringify({
            url: requestedUrl,
            raw_content: `fetch-result${claimText()}`,
          }),
        },
      ],
      isError: false,
    });
    continue;
  }
  if (toolName === "domain") {
    writeResponse(id, {
      content: [{ type: "text", text: "domain-result" }],
      structuredContent: {
        records: [
          {
            location: "上海",
            condition: "晴",
            temperature: "26",
            units: "C",
            observationTime: fixtureTimestamp,
            issueTime: fixtureTimestamp,
            title: "Synthetic title",
            publisher: "Synthetic Publisher",
            publishedAt: fixtureTimestamp,
            topic: "synthetic",
            instrument: "AAPL",
            assetKind: "equity",
            currency: "USD",
            asOf: fixtureTimestamp,
            delay: "0",
            value: "123.45",
            region: "上海",
            channel: "Synthetic Channel",
            date: fixtureDate,
            checkedAt: fixtureTimestamp,
            competition: "Synthetic League",
            participants: ["A", "B"],
            startTime: fixtureTimestamp,
            status: "scheduled",
            score: "1-0",
            sourceUrl: "https://source.invalid/domain",
            sourceTitle: "Synthetic Domain",
            observedAt: fixtureTimestamp,
            evidenceId: "provider-supplied-id",
          },
        ],
      },
      isError: false,
    });
    continue;
  }
  if (mode === "search-empty") {
    writeResponse(id, {
      content: [{ type: "text", text: "no parseable web evidence" }],
      isError: false,
    });
    continue;
  }
  writeResponse(id, {
    content: [{ type: "text", text: searchText() }],
    isError: false,
  });
}
