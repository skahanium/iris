export type McpCredentialTarget = "header" | "env";
export type McpTransportKind = "https" | "stdio";

export interface McpCredentialTemplate {
  target: McpCredentialTarget;
  name: string;
  label: string;
  service: string;
  scheme?: string;
  placeholder?: string;
  optional?: boolean;
}

export interface McpPlainEnvTemplate {
  name: string;
  label: string;
  value: string;
  placeholder?: string;
}

export interface McpProviderPreset {
  id: string;
  label: string;
  description: string;
  transportKind: McpTransportKind;
  providerName: string;
  url?: string;
  command?: string;
  args?: string[];
  allowLocalhostDev?: boolean;
  searchMapping?: string;
  fetchMapping?: string;
  credentials: McpCredentialTemplate[];
  plainEnv: McpPlainEnvTemplate[];
}

function mapping(tool: string, extra: Record<string, unknown> = {}): string {
  return JSON.stringify({ tool, ...extra });
}

export const MCP_PROVIDER_PRESETS: McpProviderPreset[] = [
  {
    id: "anysearch",
    label: "AnySearch",
    description:
      "官方 HTTPS 服务，搜索用 search，网页读取用 extract；API Key 可选，匿名额度较低。",
    transportKind: "https",
    providerName: "AnySearch",
    url: "https://api.anysearch.com/mcp",
    searchMapping: mapping("search", {
      queryArg: "query",
      maxResultsArg: "max_results",
    }),
    fetchMapping: mapping("extract", { urlArg: "url" }),
    credentials: [
      {
        target: "header",
        name: "Authorization",
        label: "AnySearch API Key",
        service: "iris.mcp.anysearch",
        scheme: "bearer",
        placeholder: "as_sk_...",
        optional: true,
      },
    ],
    plainEnv: [],
  },
  {
    id: "jina",
    label: "Jina Reader",
    description:
      "Jina MCP 远程服务；search_web 需要 API Key，read_url 可用于网页深读。",
    transportKind: "https",
    providerName: "Jina Reader",
    url: "https://mcp.jina.ai/v1",
    searchMapping: mapping("search_web", { queryArg: "query" }),
    fetchMapping: mapping("read_url", { urlArg: "url" }),
    credentials: [
      {
        target: "header",
        name: "Authorization",
        label: "Jina API Key",
        service: "iris.mcp.jina",
        scheme: "bearer",
        placeholder: "jina_...",
        optional: true,
      },
    ],
    plainEnv: [],
  },
  {
    id: "firecrawl",
    label: "Firecrawl",
    description:
      "优先使用官方免密 HTTPS 服务；需要更高额度时可改为本地命令并绑定 FIRECRAWL_API_KEY。",
    transportKind: "https",
    providerName: "Firecrawl",
    url: "https://mcp.firecrawl.dev/v2/mcp",
    searchMapping: mapping("firecrawl_search", {
      queryArg: "query",
      maxResultsArg: "limit",
    }),
    fetchMapping: mapping("firecrawl_scrape", {
      urlArg: "url",
      extraArgs: { formats: ["markdown"] },
    }),
    credentials: [],
    plainEnv: [],
  },
  {
    id: "tavily",
    label: "Tavily",
    description:
      "官方远程 MCP；Iris 使用 Authorization 请求头，不把 tavilyApiKey 写进 URL。",
    transportKind: "https",
    providerName: "Tavily",
    url: "https://mcp.tavily.com/mcp/",
    searchMapping: mapping("tavily-search", {
      queryArg: "query",
      maxResultsArg: "max_results",
    }),
    fetchMapping: mapping("tavily-extract", { urlListArg: "urls" }),
    credentials: [
      {
        target: "header",
        name: "Authorization",
        label: "Tavily API Key",
        service: "iris.mcp.tavily",
        scheme: "bearer",
        placeholder: "tvly-...",
      },
    ],
    plainEnv: [],
  },
  {
    id: "brave",
    label: "Brave Search",
    description:
      "官方本地命令服务；提供搜索能力，网页读取继续走 MCP/原生候选兜底。",
    transportKind: "stdio",
    providerName: "Brave Search",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-brave-search"],
    searchMapping: mapping("brave_web_search", { queryArg: "query" }),
    fetchMapping: undefined,
    credentials: [
      {
        target: "env",
        name: "BRAVE_API_KEY",
        label: "Brave API Key",
        service: "iris.mcp.brave",
        placeholder: "BSA...",
      },
    ],
    plainEnv: [],
  },
  {
    id: "searxng",
    label: "SearXNG",
    description:
      "使用 mcp-searxng 社区 server；需要一个启用 JSON 输出的 SearXNG 实例地址。",
    transportKind: "stdio",
    providerName: "SearXNG",
    command: "npx",
    args: ["-y", "mcp-searxng"],
    searchMapping: mapping("searxng_web_search", { queryArg: "query" }),
    fetchMapping: mapping("web_url_read", { urlArg: "url" }),
    credentials: [],
    plainEnv: [
      {
        name: "SEARXNG_URL",
        label: "SearXNG 实例地址",
        value: "",
        placeholder: "https://search.example.com",
      },
    ],
  },
];

export function findMcpProviderPreset(
  id: string | null | undefined,
): McpProviderPreset | undefined {
  return MCP_PROVIDER_PRESETS.find((preset) => preset.id === id);
}
