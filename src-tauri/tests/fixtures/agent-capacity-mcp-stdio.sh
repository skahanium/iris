#!/bin/sh
# Deterministic MCP stdio peer used only by the Rust contract tests. It relies
# exclusively on POSIX shell built-ins because Iris launches stdio MCP peers
# with a cleared environment.

mode="$1"
result_count="${2:-1}"

json_id() {
  value=${1#*\"id\":}
  value=${value%%,*}
  value=${value%%\}*}
  printf '%s' "$value"
}

if [ "$mode" = "malformed" ]; then
  printf '%s\n' 'not-json'
  exit 0
fi

while IFS= read -r line; do
  if [ "$mode" = "timeout" ]; then
    continue
  fi

  case "$line" in
    *'"method":"initialize"'*)
      id=$(json_id "$line")
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"iris-contract-mcp","version":"1"}}}\n' "$id"
      ;;
    *'"method":"tools/list"'*)
      id=$(json_id "$line")
      if [ "$mode" = "search-fetch" ]; then
        printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"}},"required":["query"],"additionalProperties":false}},{"name":"fetch","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object","properties":{"url":{"type":"string"}},"required":["url"],"additionalProperties":false}}]}}\n' "$id"
      else
        printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"}},"required":["query"],"additionalProperties":false}}]}}\n' "$id"
      fi
      ;;
    *'"method":"tools/call"'*)
      id=$(json_id "$line")
      case "$line" in
        *'"name":"fetch"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"fetch-result"}],"isError":false}}\n' "$id"
          ;;
        *)
          if [ "$mode" = "search-empty" ]; then
            printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"no parseable web evidence\"}],\"isError\":false}}"
          else
            claims=''
            ordinal=1
            while [ "$ordinal" -le 48 ]; do
              claims="$claims fact-web-$ordinal=value-$ordinal"
              ordinal=$((ordinal + 1))
            done
            if [ "$result_count" -gt 1 ]; then
              results="[1] title: Contract\\nurl: https://source.invalid/contract\\nsnippet: deterministic$claims\\n"
              index=2
              while [ "$index" -le "$result_count" ]; do
                results="$results[$index] title: Result $index\\nurl: https://source-$index.invalid/$index\\nsnippet: deterministic$claims\\n"
                index=$((index + 1))
              done
              printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"$results\"}],\"isError\":false}}"
            else
              printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"[1] title: Contract\\nurl: https://source.invalid/contract\\nsnippet: deterministic$claims\"}],\"isError\":false}}"
            fi
          fi
          ;;
      esac
      ;;
  esac
done
