# 联网搜索降级

未配置 MiniMax API Key 或 Token Plan 搜索失败时，联网搜索走 DuckDuckGo HTML 解析；配置后优先 `POST /v1/coding_plan/search`，失败再降级 DuckDuckGo。
