# QCL IPC Protocol

- Transport: any byte stream (default `stdio`; unix socket / TCP optional)
- Framing: one JSON object per line, newline-terminated, UTF-8
- Version: `api = "1"`, returned by `system.info`
- Requests and responses follow JSON-RPC 2.0

## Request

```json
{"jsonrpc":"2.0","id":1,"method":"instance.launch","params":{"id":"1.20.1-fabric"}}
```

## Response

Success (returns immediately, does not wait for the game to exit):

```json
{"jsonrpc":"2.0","id":1,"result":{"session":"s_7f3a","pid":48213}}
```

Failure:

```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"version not found: 1.99"}}
```

## Events (server → client)

Long-running operations always return a handle immediately and stream progress as events.

```json
{"method":"proc.state","params":{"session":"s_7f3a","state":"starting"}}
{"method":"log.line","params":{"session":"s_7f3a","line":{"ts":1729000000000,"level":"info","source":"game","text":"[Server thread/INFO]: Done"}}}
{"method":"download.progress","params":{"label":"assets","done":128,"total":1024}}
{"method":"proc.exit","params":{"session":"s_7f3a","code":0,"crash":null}}
```

`state` values: `starting` | `running` | `stopping` | `exited`
`level` values: `trace` | `debug` | `info` | `warn` | `error`
`source` values: `game` | `loader` | `jvm` | `wrapper`

On a crash, `crash` looks like:

```json
{"exit_code":1,"reason":"game crashed (crash report found)","hs_err":null,"crash_report":"/path/crash-reports/crash-2026-10-29.txt"}
```

## Methods

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

## Error codes

| code | Meaning |
|---|---|
| `-32700` | JSON parse failure |
| `-32601` | Unknown method |
| `-32602` | Invalid parameters |
| `-32001` | Version not found |
| `-32002` | Instance not found |
| `-32003` | Session not found |
| `-32010` | Download failed |
| `-32011` | Java runtime missing |
| `-32020` | Plugin error |

## Compatibility rules

- Clients must ignore unknown fields and events for forward compatibility
- New required parameters must bump the `api` version
- Event ordering is not guaranteed across sessions, but `log.line` is ordered within a session
