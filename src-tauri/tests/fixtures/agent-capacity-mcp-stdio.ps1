param(
    [Parameter(Position = 0)]
    [string]$Mode = "search-only",
    [Parameter(Position = 1)]
    [int]$ResultCount = 1
)

# Deterministic Windows-native counterpart to agent-capacity-mcp-stdio.sh.
# It deliberately uses only .NET console APIs so MCP contract tests can run
# without WSL, Git Bash, or an inherited PATH.
function Write-McpResponse([object]$Id, [object]$Result) {
    [Console]::Out.WriteLine((@{
        jsonrpc = "2.0"
        id = $Id
        result = $Result
    } | ConvertTo-Json -Depth 8 -Compress))
}

if ($Mode -eq "malformed") {
    [Console]::Out.WriteLine("not-json")
    exit 0
}

while (($line = [Console]::In.ReadLine()) -ne $null) {
    if ($Mode -eq "timeout") {
        continue
    }

    $idMatch = [regex]::Match($line, '"id"\s*:\s*(?:"([^"]+)"|([0-9]+))')
    if (-not $idMatch.Success) {
        continue
    }
    $id = if ($idMatch.Groups[1].Success) {
        $idMatch.Groups[1].Value
    }
    else {
        [int]$idMatch.Groups[2].Value
    }

    if ($line.Contains('"method":"initialize"')) {
        Write-McpResponse $id @{
            protocolVersion = "2025-06-18"
            capabilities = @{ tools = @{} }
            serverInfo = @{ name = "iris-contract-mcp"; version = "1" }
        }
        continue
    }

    if ($line.Contains('"method":"tools/list"')) {
        $tools = @(@{ name = "search"; inputSchema = @{ type = "object" } })
        if ($Mode -eq "search-fetch") {
            $tools += @{ name = "fetch"; inputSchema = @{ type = "object" } }
        }
        Write-McpResponse $id @{ tools = $tools }
        continue
    }

    if ($line.Contains('"method":"tools/call"')) {
        if ($line.Contains('"name":"fetch"')) {
            Write-McpResponse $id @{ content = @(@{ type = "text"; text = "fetch-result" }); isError = $false }
        }
        elseif ($Mode -eq "search-empty") {
            Write-McpResponse $id @{ content = @(@{ type = "text"; text = "no parseable web evidence" }); isError = $false }
        }
        else {
            if ($ResultCount -gt 1) {
                $text = (1..$ResultCount | ForEach-Object {
                    $claims = (1..48 | ForEach-Object { "fact-web-$_=value-$_" }) -join " "
                    "[$_] title: Result $_`nurl: https://source-$_.invalid/$_`nsnippet: deterministic $claims"
                }) -join "`n"
            }
            else {
                $claims = (1..48 | ForEach-Object { "fact-web-$_=value-$_" }) -join " "
                $text = "[1] title: Contract`nurl: https://source.invalid/contract`nsnippet: deterministic$claims"
            }
            Write-McpResponse $id @{ content = @(@{ type = "text"; text = $text }); isError = $false }
        }
    }
}
