#!/bin/sh
# Deterministic MCP stdio peer used only by the Rust contract tests. It relies
# on POSIX shell built-ins plus an absolute date command because Iris launches
# stdio MCP peers with a cleared environment.

mode="$1"
result_count="${2:-1}"
fixture_timestamp=$(/bin/date -u '+%Y-%m-%dT%H:%M:%SZ')
fixture_date=${fixture_timestamp%%T*}

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
      if [ "$mode" = "domain-dto" ]; then
        printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"search","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"}},"required":["query"],"additionalProperties":false}},{"name":"domain","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object","properties":{},"additionalProperties":false}}]}}\n' "$id"
      elif [ "$mode" = "search-fetch" ]; then
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
        *'"name":"domain"'*)
          printf '%s\n' '{"jsonrpc":"2.0","id":'"$id"',"result":{"content":[{"type":"text","text":"domain-result"}],"structuredContent":{"records":[{"location":"上海","condition":"晴","temperature":"26","units":"C","observationTime":"'"$fixture_timestamp"'","issueTime":"'"$fixture_timestamp"'","title":"Synthetic title","publisher":"Synthetic Publisher","publishedAt":"'"$fixture_timestamp"'","topic":"synthetic","instrument":"AAPL","assetKind":"equity","currency":"USD","asOf":"'"$fixture_timestamp"'","delay":"0","value":"123.45","region":"上海","channel":"Synthetic Channel","date":"'"$fixture_date"'","checkedAt":"'"$fixture_timestamp"'","competition":"Synthetic League","participants":["A","B"],"startTime":"'"$fixture_timestamp"'","status":"scheduled","score":"1-0","sourceUrl":"https://source.invalid/domain","sourceTitle":"Synthetic Domain","observedAt":"'"$fixture_timestamp"'","evidenceId":"provider-supplied-id"}]},"isError":false}}'
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
            claims="$claims date: 2026-08-18T07:00:00Z"
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
