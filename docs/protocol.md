# QCL IPC 协议

- 传输：任意字节流（默认 `stdio`，可选 unix socket / TCP）
- 帧：一行一个 JSON 对象，`\n` 结尾，UTF-8
- 版本：`api = "1"`，在 `system.info` 中返回
- 请求/响应遵循 JSON-RPC 2.0

## 请求

```json
{"jsonrpc":"2.0","id":1,"method":"instance.launch","params":{"id":"1.20.1-fabric"}}
```

## 响应

成功（立即返回，不等游戏结束）：

```json
{"jsonrpc":"2.0","id":1,"result":{"session":"s_7f3a","pid":48213}}
```

失败：

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"version not found: 1.99"}}
```

## 事件（服务端 → 客户端）

长任务一律「立即返回 handle，进度走事件」。

```json
{"method":"proc.state","params":{"session":"s_7f3a","state":"starting"}}
{"method":"log.line","params":{"session":"s_7f3a","line":{"ts":1729000000000,"level":"info","source":"game","text":"[Server thread/INFO]: Done"}}}
{"method":"download.progress","params":{"label":"assets","done":128,"total":1024}}
{"method":"proc.exit","params":{"session":"s_7f3a","code":0,"crash":null}}
```

`state` 取值：`starting` | `running` | `stopping` | `exited`
`level` 取值：`trace` | `debug` | `info` | `warn` | `error`
`source` 取值：`game` | `loader` | `jvm` | `wrapper`

崩溃时 `crash`：

```json
{"exit_code":1,"reason":"game crashed (crash report found)","hs_err":null,"crash_report":"/path/crash-reports/crash-2026-10-29.txt"}
```

## 方法表

| method | params | result |
|---|---|---|
| `version.list` | `{snapshots?:bool, limit?:int}` | `{versions:[...]}` |
| `version.install` | `{id:string, assets?:bool}` | `{session:string}` |
| `instance.list` | `{}` | `{instances:[...]}` |
| `instance.launch` | `{id:string}` | `{session:string, pid:number}` |
| `instance.attach` | `{session:string}` | `{session:string}` |
| `instance.stop` | `{session:string, graceSecs?:int}` | `{ok:true}` |
| `instance.kill` | `{session:string}` | `{ok:true}` |
| `instance.send` | `{session:string, line:string}` | `{ok:true}` |
| `auth.login` | `{mode:"offline"\|"microsoft", username?:string}` | `{name:string, uuid:string}` |
| `auth.status` | `{}` | `{mode:string, name?:string}` |
| `system.info` | `{}` | `{api:string, version:string, os:string, arch:string}` |
| `system.shutdown` | `{}` | `{ok:true}` |

## 错误码

| code | 含义 |
|---|---|
| `-32700` | JSON 解析失败 |
| `-32601` | 方法不存在 |
| `-32602` | 参数非法 |
| `-32001` | 版本不存在 |
| `-32002` | 实例不存在 |
| `-32003` | 会话不存在 |
| `-32010` | 下载失败 |
| `-32011` | Java 运行时缺失 |
| `-32020` | 插件错误 |

## 兼容规则

- 客户端必须忽略未知字段和未知事件，便于向前兼容
- 新增必填参数必须提升 `api` 版本
- 事件不保证顺序跨 session，但同一 session 内 `log.line` 保序
