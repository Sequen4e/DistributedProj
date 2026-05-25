# DCS Resource Distribution - HTTP Tracker Update Module

本项目当前实现的是小组方案中的 **HTTP Tracker 更新** 部分。Tracker 会作为一个常驻 HTTP 服务实时监听用户节点的更新请求，记录每个节点当前持有的资源索引信息，并为后续“查询请求”和“下载请求”模块预留接口。

代码全部使用 Python 标准库，不需要安装第三方依赖。

## 目录结构

```text
tracker/
  __main__.py        # 运行 Tracker 服务: python3 -m tracker
  server.py          # HTTP 实时监听服务和 API 路由
  registry.py        # 线程安全的节点/资源索引表，负责校验、记录、持久化
  client.py          # 用户节点向 Tracker 发送更新的客户端工具
examples/
  node_a_resources.json
  node_b_resources.json
tests/
  test_tracker_update.py
README.md
```

## 已完成的功能

1. Tracker 实时监听 HTTP 请求。
2. 用户节点上线或定时向 Tracker 汇报资源。
3. 更新消息包含文件哈希、文件名、块索引等信息。
4. Tracker 记录节点 IP、节点端口、最后更新时间、过期时间和资源列表。
5. 支持多个节点并发更新，内部使用锁保护索引。
6. 支持节点 TTL 自动过期清理，避免离线节点长期留在索引中。
7. 支持可选 JSON 持久化，Tracker 重启后可恢复未过期节点。
8. 提供查询/下载模块后续可复用的只读接口。

## 启动 Tracker

在项目根目录运行：

```bash
python3 -m tracker --host 127.0.0.1 --port 8000 --state tracker_state.json --ttl 90
```

如果需要局域网中其他电脑访问 Tracker，启动时把 host 改成 `0.0.0.0`：

```bash
python3 -m tracker --host 0.0.0.0 --port 8000 --state tracker_state.json --ttl 90
```

参数说明：

- `--host`：Tracker 监听地址。演示单机用 `127.0.0.1`，多电脑演示用 `0.0.0.0`。
- `--port`：Tracker 监听端口，默认 `8000`。
- `--state`：Tracker 状态持久化文件，默认 `tracker_state.json`。
- `--no-state`：不保存状态，只使用内存。
- `--ttl`：节点超过多少秒没有更新就被认为离线，默认 `90` 秒。
- `--cleanup-interval`：后台清理过期节点的间隔，默认 `10` 秒。

启动成功后会看到：

```text
HTTP Tracker listening on http://127.0.0.1:8000
State file: tracker_state.json
Press Ctrl+C to stop.
```

## 用户节点发送更新

示例资源文件在 `examples/node_a_resources.json`：

```json
{
  "resources": [
    {
      "file_hash": "sha256-file-x-demo",
      "file_name": "file-x.txt",
      "file_size": 3072,
      "block_size": 1024,
      "blocks": [
        {"index": 0, "hash": "sha256-file-x-block-0", "size": 1024},
        {"index": 1, "hash": "sha256-file-x-block-1", "size": 1024}
      ]
    }
  ]
}
```

发送一次更新：

```bash
python3 -m tracker.client \
  --tracker http://127.0.0.1:8000 \
  --node-id node-a \
  --host 127.0.0.1 \
  --port 9001 \
  --resources examples/node_a_resources.json
```

这里的 `--port 9001` 不是 Tracker 的端口，而是该用户节点后续用于给其他节点下载文件块的端口。下载模块同学可以在这个端口上实现自己的 peer-to-peer 文件块传输服务。

再模拟第二个节点：

```bash
python3 -m tracker.client \
  --tracker http://127.0.0.1:8000 \
  --node-id node-b \
  --host 127.0.0.1 \
  --port 9002 \
  --resources examples/node_b_resources.json
```

## 更新 API 设计

### `POST /api/v1/update`

用户节点上线、资源变化、或定时心跳时调用。这个接口是本任务的核心。

请求体：

```json
{
  "node_id": "node-a",
  "host": "127.0.0.1",
  "port": 9001,
  "resources": [
    {
      "file_hash": "sha256-file-x-demo",
      "file_name": "file-x.txt",
      "file_size": 3072,
      "block_size": 1024,
      "blocks": [
        {"index": 0, "hash": "sha256-file-x-block-0", "size": 1024},
        {"index": 1, "hash": "sha256-file-x-block-1", "size": 1024}
      ]
    }
  ],
  "metadata": {
    "client": "python-node"
  }
}
```

字段说明：

- `node_id`：节点唯一 ID，例如 `node-a`、`node-1`。
- `host`：用户节点对其他节点开放的 IP 或主机名。
- `port`：用户节点对其他节点开放的下载端口。
- `resources`：该节点当前持有的完整资源列表。
- `file_hash`：文件哈希，用于唯一识别文件。
- `file_name`：展示给用户看的文件名。
- `file_size`：可选，文件总大小。
- `block_size`：可选，每个块的大小。
- `blocks`：该节点持有的块索引。可以写 `[0, 1, 2]`，也可以写成带块哈希的对象列表。
- `metadata`：可选，给后续功能保留的额外信息。

响应体：

```json
{
  "status": "ok",
  "node_id": "node-a",
  "registered_resources": 2,
  "known_files": 2,
  "expires_at": "2026-05-23T12:00:00Z",
  "server_time": "2026-05-23T11:58:30Z"
}
```

注意：每次更新都是该节点资源列表的完整替换。也就是说，如果 `node-a` 第二次只上报一个文件，Tracker 会认为它现在只持有这一个文件。

### `POST /api/v1/offline`

用户节点正常退出时可以调用，让 Tracker 立刻移除该节点。

```json
{
  "node_id": "node-a"
}
```

### `GET /health`

健康检查，确认 Tracker 正在实时监听。

```bash
curl http://127.0.0.1:8000/health
```

## 预留给后续同学的接口

这些接口已经可以读取 Tracker 当前索引，但下载策略、真正的文件块传输不在本任务内。

### `GET /api/v1/files`

返回 Tracker 当前知道的文件列表：

```bash
curl http://127.0.0.1:8000/api/v1/files
```

返回内容包括：

- 文件哈希
- 文件名
- 文件大小
- 块大小
- 当前持有该文件的节点数量
- 已知可用块索引

### `GET /api/v1/query?file_hash=...`

按文件哈希查询文件位置，供“查询请求”和“下载请求”模块使用：

```bash
curl "http://127.0.0.1:8000/api/v1/query?file_hash=sha256-file-x-demo"
```

返回结构中的 `locations[].peers[]` 会包含：

- `node_id`
- `host`
- `port`
- `blocks`

下载模块可以根据这些信息实现负载均衡，例如从 node-a 下载块 0，从 node-b 下载块 2。

### `GET /api/v1/query?file_name=...`

按文件名查询：

```bash
curl "http://127.0.0.1:8000/api/v1/query?file_name=file-x.txt"
```

因为不同文件可能同名但哈希不同，真正下载时建议优先使用 `file_hash`。

### `GET /api/v1/nodes`

查看当前在线节点：

```bash
curl http://127.0.0.1:8000/api/v1/nodes
```

### `GET /api/v1/snapshot`

调试用，返回节点和文件的完整快照：

```bash
curl http://127.0.0.1:8000/api/v1/snapshot
```

## 后续模块如何接入

用户节点同学可以直接调用：

```python
from tracker.client import send_update

send_update(
    "http://127.0.0.1:8000",
    node_id="node-a",
    host="127.0.0.1",
    port=9001,
    resources=[
        {
            "file_hash": "sha256-file-x-demo",
            "file_name": "file-x.txt",
            "blocks": [0, 1]
        }
    ],
)
```

查询模块同学可以直接使用 HTTP 接口，或在同一进程中使用：

```python
registry.find_file_locations(file_hash="sha256-file-x-demo")
```

下载模块同学应该使用 `GET /api/v1/query` 返回的 `host`、`port` 和 `blocks` 来计算下载计划。真正的数据传输接口建议由用户节点实现，例如：

```text
GET http://<peer-host>:<peer-port>/api/v1/blocks?file_hash=<hash>&block_index=<index>
```

上面这个 peer 下载接口目前没有实现，因为它属于后续同学的“文件块传输/下载请求”任务。

## 测试

运行：

```bash
python3 -m unittest discover -s tests -v
```

当前测试覆盖：

- 节点资源更新后 Tracker 能建立文件索引。
- 缺少块索引的非法更新会被拒绝。
- 节点超过 TTL 会被清理。
- HTTP 服务能监听、接收更新并通过查询接口返回节点位置。

## 演示建议

1. 启动 Tracker。
2. 启动或模拟至少 3 个用户节点，分别发送 `/api/v1/update`。
3. 打开 `/api/v1/files` 展示 Tracker 已记录文件索引。
4. 打开 `/api/v1/query?file_hash=...` 展示某个文件在哪些节点、哪些块可用。
5. 说明后续下载模块会根据 Tracker 返回的节点和块索引执行并行下载。

这能清楚对应项目计划里的 “Tracker 监听用户主动发送的更新消息，并记录文件哈希、文件名、块索引” 这一部分。
