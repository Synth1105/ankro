# ankro

`ankro` is a TCP bridge that sits between a browser-facing app and a target process.
It accepts requests, decides whether they should be executed immediately or queued, and forwards them to a target executable that understands a small `-b` / `-r` control protocol.

The repository includes a complete example application under `example-app/` that demonstrates the full flow:

`browser -> example bridge -> ankro -> target`

## What Problem It Solves

`ankro` is useful when you want one process to:

- serialize access to an expensive or fragile target
- apply backpressure under bursty load
- keep a persistent queue of deferred requests
- ban callers after repeated abuse from the same IP
- expose a very small protocol boundary around a local executable

The project is intentionally small and explicit. It is designed to be easy to reason about rather than to act as a general-purpose proxy.

## Architecture

At runtime, `ankro` has four moving parts:

1. A TCP listener that accepts bridge requests.
2. A request queue split into normal and banned lanes.
3. A ban list that counts requests per source IP.
4. A target process that responds to two control flags:
   - `-b` means "busy check"
   - `-r <payload>` means "run the request"

The service path is:

1. Read one request from the socket.
2. Decide whether to queue it.
3. If the request runs immediately, spawn the target with `-r`.
4. If the request is queued, persist it to disk and wait for the queue consumer.
5. The queue consumer waits until the target is idle, then replays the oldest queued request.

## Request Protocol

`ankro` expects a text request made of newline-delimited `key=value` lines, followed by a blank line.

The example app uses this shape:

```text
id=burst-1
source=burst
payload=burst-1
createdAt=2026-06-21T12:00:00.000Z

```

The target executable receives the request body as a single comma-separated argument after `-r`.
For example:

```bash
target -r id=burst-1,source=burst,payload=burst-1,createdAt=...
```

The target must also support:

- `target -b`
- `target -r <payload>`

If `-b` prints anything, `ankro` treats the target as busy.

## Quick Start

From the repository root:

```bash
cargo build --release
```

Then start the example target:

```bash
./example-app/target
```

Start `ankro` and point it at that target:

```bash
./target/release/ankro serve --port 1234 --target ./example-app/target --ban-threshold 1000
```

Start the browser-facing bridge:

```bash
node example-app/server.mjs
```

Open:

```text
http://127.0.0.1:5555
```

If you want the bridge on a different port, set `EXAMPLE_APP_PORT`.

## CLI

`ankro` currently exposes one command:

```bash
ankro serve --port 1234 --target ./example-app/target --ban-threshold 1000
```

Flags:

- `--port` sets the TCP port for `ankro` itself. Default: `1234`
- `--target` points to the target executable. This can be an absolute path, a relative path, or a command on `PATH`
- `--ban-threshold` controls how many requests a source IP can send before it is banned. Default: `1000`

## Queueing Semantics

`ankro` keeps two in-memory queues:

- `normal` for regular traffic
- `banned` for traffic from clients that crossed the ban threshold

Rules:

- normal traffic is always drained before banned traffic
- if the target is busy, incoming requests are queued
- queued requests are persisted to `/tmp/ankro/queue.json`
- queue state is reloaded on startup

The queue consumer runs in the background and replays requests when the target reports idle.

## Target Resolution

`ankro` resolves the `--target` argument before starting the listener.

Resolution behavior:

- if the value looks like a path, it is checked directly
- otherwise `ankro` checks the raw value, the current working directory, and `PATH`
- if the path is a directory, startup fails with a clear error
- if a file is found, `ankro` canonicalizes it to an absolute path before use

This avoids surprises when the process is launched from a different directory than the one where the target lives.

## Logging

The binary initializes `tracing_subscriber` and emits operational logs with `tracing`.

Useful signals:

- server startup
- queue consumer activity
- target probe failures
- request handling errors
- ban decisions

When debugging queueing or target issues, run with a verbose tracing filter if needed.

## Environment Variables

The example application uses these environment variables:

- `EXAMPLE_APP_PORT` sets the browser bridge port. Default: `5555`
- `ANKRO_HOST` sets the host where the bridge connects to `ankro`. Default: `127.0.0.1`
- `ANKRO_PORT` sets the `ankro` TCP port. Default: `1234`
- `ANKRO_EXAMPLE_LATENCY_MS` sets artificial latency in the example target. Default: `240`

## Troubleshooting

### `Too many open files`

This usually means the bridge is generating more concurrent requests than the process limit can support.
The live socket count is bounded inside `ankro`, but very large bursts may still require increasing the OS file descriptor limit or lowering the request rate.

### `Broken pipe`

This usually means the client side timed out or closed the socket before `ankro` wrote a response.
For the example app, this is most often caused by an overly aggressive timeout on the bridge side.

### `connection error: No such file or directory`

This usually means the target path is wrong, the binary is not executable, or the relative path was resolved from the wrong working directory.
Use an absolute path if in doubt.

### Queue file errors under `/tmp/ankro`

If `/tmp/ankro` cannot be created or written, `ankro` will fail while persisting queue state.
Make sure the process can write to `/tmp`.

## Development

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for setup, coding standards, testing, and contribution workflow.

## Example App

The example app is documented separately in [`example-app/README.md`](./example-app/README.md).
It is the best place to start if you want to understand the full browser-to-target flow.

