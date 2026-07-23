#!/bin/sh
# Deterministic MCP stdio peer used only by the Rust contract tests. It relies
# exclusively on POSIX shell built-ins because Iris launches stdio MCP peers
# with a cleared environment.

mode="$1"

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
        printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","inputSchema":{"type":"object"}},{"name":"fetch","inputSchema":{"type":"object"}}]}}\n' "$id"
      else
        printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","inputSchema":{"type":"object"}}]}}\n' "$id"
      fi
      ;;
    *'"method":"tools/call"'*)
      id=$(json_id "$line")
      case "$line" in
        *'"name":"fetch"'*)
          printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"fetch-result"}],"isError":false}}\n' "$id"
          ;;
        *)
          printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"[1] title: Contract\\nurl: https://source.invalid/contract\\nsnippet: deterministic\"}],\"isError\":false}}"
          ;;
      esac
      ;;
  esac
done
