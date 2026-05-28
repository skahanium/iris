# 凭据安全

API Key 不得写入日志、SQLite settings 或仓库内明文文件。

LLM Key 存入 `iris.llm.{provider}`；MiniMax Token Plan 检索 Key 存入 `iris.minimax`。禁止明文写入日志、数据库或配置文件。
