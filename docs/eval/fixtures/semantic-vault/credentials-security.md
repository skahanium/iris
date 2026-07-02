# 凭据安全

API Key 不得写入日志、SQLite settings 或仓库内明文文件。

LLM Key 存入 `iris.llm.{provider}`；MCP 工具凭据引用系统凭据服务 `iris.mcp.*`。禁止明文写入日志、数据库或配置文件。
