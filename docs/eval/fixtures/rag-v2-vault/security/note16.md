---
title: "OAuth2客户端凭证模式的安全边界"
aliases: ["OAuth2客户端凭证", "client-credentials-grant"]
tags: ["area-security", "fixture", "应用安全", "OAuth2", "客户端凭证"]
---

# OAuth2客户端凭证模式的安全边界

OAuth2 客户端凭证模式（Client Credentials Grant）是 OAuth2 协议中设计最为简洁的授权方式，专用于服务间通信（Machine-to-Machine, M2M）场景。与授权码流程需要用户参与不同，客户端凭证模式假定客户端本身就是资源所有者，直接使用 client_id 和 client_secret 向授权服务器的令牌端点请求访问令牌。

客户端凭证模式的安全性完全取决于客户端密钥的保密性。由于没有用户登录环节，该模式不产生刷新令牌，客户端在令牌过期后直接重新发起客户端凭证请求即可获取新令牌。这种设计简化了令牌生命周期管理，但也意味着客户端密钥一旦泄露，攻击者可以无限期获取新令牌来访问受保护资源。

证据令牌: evaltok16

客户端身份认证方式在 OAuth2 中有多种选择。client_secret_post 将密钥放在请求体中，client_secret_basic 使用 HTTP Basic Authentication 传递，而 private_key_jwt 和 mTLS（双向 TLS）则利用非对称加密提供更高的安全保证。对于运行在不受信任网络中的服务，强烈建议使用 mTLS 或 private_key_jwt 替代共享密钥方案。

客户端凭证模式的适用边界需要严格界定。它不适合任何涉及用户数据访问的场景，因为该模式无法传递用户身份上下文。在微服务架构中，客户端凭证模式常用于服务网格内部的资源访问授权，如 API 网关向用户服务查询数据。但在这种场景下，应当配合令牌绑定（Token Binding）和源 IP 白名单等额外控制措施来降低令牌泄漏风险。
