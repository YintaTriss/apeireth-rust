// apeireth-sdk C-ABI header (R122-8 auto-generated, 0 改 24 LOCKED)
// O-5 实质: 0 假装 100% multi-lang, 仅 5 fn demo 桥接.
// 0 改 workspace.version 1.1.0, 0 触碰 11 agent 公共 API 签名.
// Skeleton 桥接 1:1 c.rs 5 fn (count_tokens_c / hash_request_c /
// version_c / compile_info_c / free_string_c).
// 编译指令: cargo build -p apeireth-sdk --features c


#ifndef APEIRETH_SDK_H
#define APEIRETH_SDK_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define SDK_SUBMODULE_COUNT 4

/**
 * K-1 强校验: `SDK_TOOL_WHITELIST` 长度 == 8 (6 工具 + 2 通用).
 */
#define SDK_TOOL_WHITELIST_COUNT 8

/**
 * **STUB MODE 守门标志** (K-1 强校验 #4): 编译期 hardcode = `true`.
 *
 * R21 真接 `apeireth-api` HTTP/WS 时, **必须经 8 哲学锚 (S-1/S-2/S-3 质量工程化 NEW/O-1 安全优先 NEW/O-2/O-3/O-4/O-5, baseline 2026-08-19)
 * + 主人审才能改 `false`**.
 */
#define STUB_MODE true

/**
 * API key 最小长度 (16, 防过短 key 误匹配).
 */
#define API_KEY_MIN_LENGTH 16

/**
 * API key 最大长度 (4 KB, 跟 `apeireth-keyring::TOKEN_MAX_LENGTH` 1:1).
 */
#define API_KEY_MAX_LENGTH 4096

/**
 * 客户端 token bucket 容量 (P0 端点 1000 req/s, 普通 100 req/s, per D-04).
 */
#define CLIENT_BUCKET_CAPACITY 1000.0

/**
 * 客户端 token bucket 填充速率 (1000 token/s, 即 1000 req/s).
 */
#define CLIENT_BUCKET_REFILL_PER_SEC 1000.0

/**
 * 编译期守门: LARK_TOOL_WHITELIST 长度 == 8 (K-1 强校验 + 8 项不修改承诺 #5).
 */
#define LARK_TOOL_WHITELIST_COUNT 8

/**
 * 编译期守门: 8 核心 API 数 == LARK_TOOL_WHITELIST_COUNT (K-1 强校验同步守门).
 */
#define CORE_API_COUNT 8

/**
 * 6 核心 API 数量常量 (per task spec §3 + v0.9.21 商业版 1:1).
 */
#define CORE_API_COUNT 6

/**
 * 6 核心 API 数量常量 (per task spec §3 + v0.9.21 商业版 1:1).
 */
#define CORE_API_COUNT 6

/**
 * 6 消息类型守门常量 (per K-1 强校验守门, 编译期 hardcode).
 */
#define MESSAGE_TYPE_COUNT 6

/**
 * 5 鉴权守门常量 (per K-1 强校验守门, 编译期 hardcode).
 */
#define AUTH_METHOD_COUNT 5

/**
 * 4 实体守门常量 (per K-1 强校验守门, 编译期 hardcode).
 */
#define ENTITY_COUNT 4

/**
 * 6 K-1 强校验守门常量 (per K-1 强校验守门, 编译期 hardcode).
 */
#define K1_STRONG_VALIDATION_COUNT 6

/**
 * 4 K-1 强校验 数量常量 (per task spec §3 + K-1 守门).
 */
#define K1_STRONG_VALIDATION_COUNT 4

/**
 * 6 K-1 强校验 数量常量 (per task spec §3 + K-1 守门).
 */
#define K1_STRONG_VALIDATION_COUNT 6

/**
 * 单消息最大文本长度 (per v0.9.21 商业版估 4 KiB, 防单消息爆炸).
 */
#define MAX_MESSAGE_TEXT_BYTES 4096

/**
 * 单次 list_calendar_events 最大返回数 (per v0.9.21 商业版估 1000).
 */
#define MAX_CALENDAR_EVENTS_PER_PAGE 1000

/**
 * 单 webhook 单 chunk 字节上限 (per v0.9.21 商业版估 16 KiB, R21 续真接 AES).
 */
#define MAX_WEBHOOK_CHUNK_BYTES (16 * 1024)



/**
 * 5 状态 hardcode 常量.
 */
#define InstanceStatus_COUNT 5

/**
 * 3 状态 hardcode 常量.
 */
#define TaskStatus_COUNT 3

/**
 * 默认 tenant_access_token TTL (2h = 7200s, per 飞书 Open Platform 文档).
 */
#define DEFAULT_TENANT_TOKEN_TTL_SECONDS 7200

/**
 * 默认 user_access_token TTL (2h = 7200s, per 飞书 Open Platform OAuth 文档).
 */
#define DEFAULT_USER_TOKEN_TTL_SECONDS 7200

/**
 * Token 最大 TTL (24h, per 飞书 Open Platform 上限, 防长占).
 */
#define MAX_TOKEN_TTL_SECONDS 86400

/**
 * Token 最大 TTL (24h, per livekit-server 上限).
 */
#define MAX_TOKEN_TTL_SECONDS 86400

/**
 * Token 最大 TTL (24h, per Anthropic API 上限, 防长占).
 */
#define MAX_TOKEN_TTL_SECONDS 86400

/**
 * App ID 最小长度 (cli_ + 8 char = 12, per 飞书规范).
 */
#define MIN_APP_ID_LENGTH 12

/**
 * App Secret 最小长度 (per 飞书规范, 16 char).
 */
#define MIN_APP_SECRET_LENGTH 16

/**
 * App Secret 典型长度 (32 char, per 飞书默认).
 */
#define TYPICAL_APP_SECRET_LENGTH 32

/**
 * 5 状态 hardcode 常量.
 */
#define EventStatus_COUNT 5

/**
 * 3 variant hardcode 常量.
 */
#define DocumentType_COUNT 3

/**
 * 编译期守门: 11 variant 守门 (per 8 项不修改承诺).
 * 新增 variant 必须同步改本 const, 强行提醒 reviewer.
 */
#define LARK_ERROR_VARIANT_COUNT 11

/**
 * 6 类型 hardcode 常量.
 */
#define MessageType_COUNT 6

/**
 * 4 variant hardcode 常量.
 */
#define EventType_COUNT 4

/**
 * 5 RoomState 数量常量 (per `SUPPORTED_ROOM_STATES.len()`).
 */
#define ROOM_STATE_COUNT 5

/**
 * 8 RoomEvent 数量常量 (per `SUPPORTED_ROOM_EVENTS.len()`).
 */
#define ROOM_EVENT_COUNT 8

/**
 * LiveKit 默认事件 channel 容量 (per v0.9.21 商业版 Room 内部, 100 events).
 */
#define EVENT_CHANNEL_CAPACITY 100

/**
 * 编译期守门: TOOL_WHITELIST 长度 == 7 (6 核心 API + 1 stub_status).
 */
#define TOOL_WHITELIST_COUNT 7

/**
 * 编译期守门: TOOL_WHITELIST 长度 == 7 (6 核心 API + 1 stub_status).
 */
#define TOOL_WHITELIST_COUNT 7

/**
 * 默认 access token TTL (1h, per livekit-server 默认).
 */
#define DEFAULT_TOKEN_TTL_SECONDS 3600

/**
 * 默认 access token TTL (1h = 3600s, per Anthropic API 文档).
 */
#define DEFAULT_TOKEN_TTL_SECONDS 3600

/**
 * 8 事件 hardcode 常量.
 */
#define RoomEvent_COUNT 8

/**
 * 4 等级 + 1 unknown = 5 variant (per livekit-client v0.9.21 实际 5 variant).
 */
#define ConnectionQuality_COUNT 5

/**
 * 5 权限 hardcode.
 */
#define Permission_COUNT 5

/**
 * 5 状态机 hardcode 常量.
 */
#define RoomState_COUNT 5

/**
 * 2 类型 hardcode.
 */
#define TrackKind_COUNT 2

/**
 * 5 variant (4 known + 1 unknown, per livekit-client v0.9.21).
 */
#define TrackSource_COUNT 5

/**
 * 编译期守门: SANDBOX_TOOL_WHITELIST 长度 == 6 (K-1 强校验 + 8 项不修改承诺 #5).
 */
#define SANDBOX_TOOL_WHITELIST_COUNT 6

/**
 * 单沙箱最大存活时间 (秒, 1h, per v0.9.21 商业版估, 防恶意沙箱长占资源).
 */
#define SANDBOX_MAX_LIFETIME_SECONDS 3600

/**
 * 单次 streamLogs 最大 chunk 数 (per v0.9.21 商业版估 10000, 防 stream 爆炸).
 */
#define SANDBOX_MAX_LOG_CHUNKS 10000

/**
 * 单 chunk 字节上限 (4 KiB, per v0.9.21 商业版估, 防单 log line 爆炸).
 */
#define SANDBOX_MAX_LOG_CHUNK_BYTES 4096

/**
 * 编译期守门: 10 variant 守门 (per R20 5 P0 风格 + 8 项不修改承诺).
 * 新增 variant 必须同步改本 const, 强行提醒 reviewer.
 */
#define SANDBOX_ERROR_VARIANT_COUNT 10

/**
 * 单沙箱最大 env 变量数 (per v0.9.21 商业版估 64, 防 env 爆炸).
 */
#define MAX_ENV_VARS 64

/**
 * 单沙箱最大卷挂载数 (per v0.9.21 商业版估 32).
 */
#define MAX_VOLUME_MOUNTS 32

/**
 * 单沙箱最大端口映射数 (per v0.9.21 商业版估 16).
 */
#define MAX_PORT_MAPPINGS 16

/**
 * 最小 CPU 核数 (per v0.9.21 商业版估 0.1, 防止过度限制导致进程无法启动).
 */
#define MIN_CPU_CORES 0.1

/**
 * 最大 CPU 核数 (per v0.9.21 商业版估 64, 防止独占宿主机).
 */
#define MAX_CPU_CORES 64.0

/**
 * 最小内存 (16 MiB, per v0.9.21 商业版估, 防止进程无法启动).
 */
#define MIN_MEMORY_BYTES ((16 * 1024) * 1024)

/**
 * 最大内存 (256 GiB, per v0.9.21 商业版估, 防止 OOM 宿主机).
 */
#define MAX_MEMORY_BYTES (((256 * 1024) * 1024) * 1024)

/**
 * 最小 IO 带宽 (1 MiB/s, per v0.9.21 商业版估).
 */
#define MIN_IO_BANDWIDTH_BPS (1024 * 1024)

/**
 * 最大 IO 带宽 (10 GiB/s, per v0.9.21 商业版估).
 */
#define MAX_IO_BANDWIDTH_BPS (((10 * 1024) * 1024) * 1024)

/**
 * 最小网络带宽 (1 MiB/s, per v0.9.21 商业版估).
 */
#define MIN_NET_BANDWIDTH_BPS (1024 * 1024)

/**
 * 最大网络带宽 (10 GiB/s, per v0.9.21 商业版估).
 */
#define MAX_NET_BANDWIDTH_BPS (((10 * 1024) * 1024) * 1024)

/**
 * 最小临时目录大小 (1 MiB, per v0.9.21 商业版估).
 */
#define MIN_TMP_BYTES (1024 * 1024)

/**
 * 最大临时目录大小 (100 GiB, per v0.9.21 商业版估).
 */
#define MAX_TMP_BYTES (((100 * 1024) * 1024) * 1024)

/**
 * 编译期守门: 5 SandboxStatus 守门 (1:1 翻译 v0.9.21 商业版状态机).
 */
#define SANDBOX_STATUS_COUNT 6

/**
 * 4 STT 模型 数量常量 (per `SUPPORTED_STT_MODELS.len()`).
 */
#define STT_MODEL_COUNT 4

/**
 * 4 TTS 模型 数量常量 (per `SUPPORTED_TTS_MODELS.len()`).
 */
#define TTS_MODEL_COUNT 4

/**
 * 4 唤醒词类别 数量常量 (per `SUPPORTED_WAKE_WORD_CATEGORIES.len()`).
 */
#define WAKE_WORD_CATEGORY_COUNT 4

/**
 * 3 VAD 算法 数量常量 (per `SUPPORTED_VAD_ALGORITHMS.len()`).
 */
#define VAD_ALGORITHM_COUNT 3

/**
 * 默认 audio session 容量 (per v0.9.21 商业版估 100 sessions).
 */
#define SESSION_CHANNEL_CAPACITY 100

/**
 * API Key 最小长度 (per K-1 #1 强校验, 16 char).
 */
#define MIN_API_KEY_LENGTH 16

/**
 * API Key 典型长度 (32 char, per Anthropic voice 规范).
 */
#define TYPICAL_API_KEY_LENGTH 32

/**
 * VoiceConfig 段数 (per task spec §1, 编译期 hardcode 5).
 */
#define VOICE_CONFIG_SECTION_COUNT 5

/**
 * 默认采样率 (16kHz, per Porcupine 官方 + v0.9.21 商业版估).
 */
#define DEFAULT_AUDIO_SAMPLE_RATE 16000

/**
 * 默认位深 (16-bit, per v0.9.21 商业版估).
 */
#define DEFAULT_AUDIO_BIT_DEPTH 16

/**
 * 默认通道数 (单声道, per v0.9.21 商业版估).
 */
#define DEFAULT_AUDIO_CHANNELS 1

/**
 * 编译期守门: 12 variant 守门 (per 8 项不修改承诺).
 * 新增 variant 必须同步改本 const, 强行提醒 reviewer.
 */
#define VOICE_ERROR_VARIANT_COUNT 12

/**
 * 4 模型 hardcode 常量.
 */
#define SttModel_COUNT 4

/**
 * 4 模型 hardcode 常量.
 */
#define TtsModel_COUNT 4

/**
 * 3 算法 hardcode 常量.
 */
#define VadAlgorithm_COUNT 3

/**
 * 自定义唤醒词最大长度 (per v0.9.21 商业版估 64 char, 防恶意长串).
 */
#define MAX_CUSTOM_WAKE_WORD_LENGTH 64

/**
 * 唤醒词最小长度 (per v0.9.21 商业版估 3 char, 防过短误触).
 */
#define MIN_WAKE_WORD_LENGTH 3

/**
 * 4 类别 hardcode 常量.
 */
#define WakeWordCategory_COUNT 4

/**
 * 沙箱隔离级别 (3 variant, 1:1 翻译 @anthropic-ai/sandbox 商业版 v0.9.21).
 *
 * K-1 强校验 #3: 编译期 hardcode, 不允许运行时增删 variant.
 */
typedef struct IsolationLevel IsolationLevel;

/**
 * 沙箱运行时 (3 variant, 1:1 翻译 @anthropic-ai/sandbox 商业版 v0.9.21).
 *
 * K-1 强校验 #2: 编译期 hardcode, 不允许运行时增删 variant.
 */
typedef struct RuntimeKind RuntimeKind;

/**
 * 审批任务状态 (3 variant, per v0.9.21 商业版 `status` 字段).
 */
typedef struct TaskStatus TaskStatus;

/**
 * Lark API schema version (1:1 翻译 @larksuiteoapi/lark-sdk v0.9.21, K-1 强校验).
 *
 * 跟 `LARK_SCHEMA_VERSION` (in auth.rs) 同步, 此处 re-export 守门防漂移.
 */
#define LARK_API_VERSION LARK_SCHEMA_VERSION

/**
 * 默认 Lark API base URL.
 */
#define DEFAULT_API_BASE DEFAULT_LARK_API_BASE







/**
 * Stub for the negotiation entry point — full negotiation in V2 D2.
 */
int32_t apeireth_sdk_init(void);

/**
 * Stub for error-message retrieval — last-error buffer wired in V2 D2.
 */
int32_t apeireth_sdk_last_error(uint8_t *_buf, uintptr_t _len);

/**
 * **C-ABI fn #1**: `apeireth_sdk_count_tokens(text: *const c_char) -> c_uint`.
 *
 * 安全性: caller 须保证 `text` 指向有效 UTF-8 + null-terminated C string.
 * Null / invalid ptr 返 0 (fail-soft, 1:1 abi.rs stub pattern).
 */
unsigned int apeireth_sdk_count_tokens(const char *text);

/**
 * **C-ABI fn #2**: `apeireth_sdk_hash_request(method, url, body, body_len) -> *mut c_char`.
 *
 * **内存契约**: caller **必须**用 `apeireth_sdk_free_string` 释放返值, 0 用 C free().
 * Null ptr 返 null. invalid UTF-8 返 null.
 */
char *apeireth_sdk_hash_request(const char *method,
                                const char *url,
                                const unsigned int *body,
                                uintptr_t body_len);

/**
 * **C-ABI fn #3**: `apeireth_sdk_version() -> *const c_char`.
 *
 * **不漂移**: 复用 `apeireth_sdk::version::SDK_VERSION` 公共 API, 0 改 workspace.version 1.2.0 (双轴制: 产品轴 tag v1.0.0 + workspace 轴 1.2.0).
 * 返 Rust static str, 生命周期 'static, 0 需要 free (1:1 libc `getenv` pattern).
 */
const char *apeireth_sdk_version(void);

/**
 * **C-ABI fn #4**: `apeireth_sdk_compile_info() -> *const c_char`.
 *
 * 返 "rustc X.Y.Z target triple, apeireth-sdk features: `[python,node,c,default]`" 字面量.
 * 0 假装实际 rustc version (编译期 hardcode "unknown" + "cfg(apeireth_sdk)" marker).
 */
const char *apeireth_sdk_compile_info(void);

/**
 * **C-ABI fn #5**: `apeireth_sdk_free_string(ptr: *mut c_char)`.
 *
 * 释放 `apeireth_sdk_hash_request` / `apeireth_sdk_version` / `apeireth_sdk_compile_info`
 * 返的 C string. 0 是 malloc 返值调 free() 行为未定义.
 */
void apeireth_sdk_free_string(char *ptr);

#endif  /* APEIRETH_SDK_H */
