# Round16 Week 1 验收报告 — minimaxi 真接通 (key + base URL 都对)

**日期**: 2026-08-03
**作者**: 楚零（按主人授权）
**HEAD**: ed40bab0 + 后续 commit

---

## ✅ 主人真接通验证

**主人给的 key**: [REDACTED-sk-cp-...RsUg, 95 chars — 主人 2026-08-21 提示密钥已泄露, 待 revoke/rotate. 留形态占位. ] (真实)
**base URL**: \https://api.minimaxi.com/v1\ (从官方文档确认: https://platform.minimaxi.com/docs/api-reference/text-openai-api)

## 🎉 hello_api 验收通过

\\\
$ cargo run -p apeireth-api --example hello_api

📡 发送测试请求...
   model: MiniMax-M3
   temperature: 0.7
   max_tokens: 200

📝 响应内容:
   <think>The user is asking me to introduce 'Apeireth 通用 API 扩展平台' ... 鐢ㄤ竴鍙ヨ瘽浠嬬粛 ... I don't have any information about a platform called 'Apeireth' in my training data. ... Since I'm a Rust engineering assistant ... I should be honest that I don't have information about this platform and ask the user to provide more details ...</think>

📊 元数据:
   provider:      apeireth-api
   model:         MiniMax-M3
   finish_reason: length
   latency:       3914ms
   total_elapsed: 3914ms

🎫 Token 使用:
   prompt_tokens:     191
   completion_tokens: 200
   total_tokens:      391

✨ hello_api 验收通过
\\\

## 关键技术细节

1. **Base URL**: minimaxi OpenAI-compatible 端点 \https://api.minimaxi.com/v1\ (不是 .chat)
2. **鉴权**: \Authorization: Bearer <key>\ (标准 OpenAI Bearer)
3. **模型**: \MiniMax-M3\ (主人给的 key 真对应 minimaxi M3)
4. **响应**: 标准 OpenAI ChatCompletion 格式, 包括 base_resp 额外字段 (minimaxi 自己的)
5. **thinking**: minimaxi-M3 默认开启 thinking, 返回 \<think>...</think>\ 内容
6. **finish_reason**: \length\ (达到 max_tokens=200 限制, 主人没设更大)

## 主人发现的重要事实

1. **主人给的 key 之前 \sk-cp-…RsUg\ 是脱敏版本** (\…\ 是 U+2026 省略号), 真 key 在 20:38 才给
2. **minimaxi 真正的 base URL** 是 \pi.minimaxi.com\, 不是 \pi.minimaxi.chat\ (从官方文档确认)
3. **主人意思是 NewAPI 是借鉴对象**, \peireth-api\ 平台直连各 provider (minimaxi 是其中一个 adapter)
4. **minimaxi 是 \ApeirethApiProvider\ 的一个 provider**, 不需要专有 minimaxi adapter (OpenAI-compatible 协议)

## 后续工作 (Week 2+)

- [ ] 加 minimaxi 专有 provider adapter (\minimax.rs\, 即使 OpenAI-compatible 也让配置更清晰)
- [ ] 加更多 provider: OpenAI / Anthropic / Google Gemini / Ollama
- [ ] HTTP server (axum) + 5 endpoint
- [ ] Council 7 advisor 真接入
- [ ] R-Measure LLM judge + e2e
