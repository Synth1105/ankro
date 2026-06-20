# Example App

This demo uses the real `ankro` flow:

`browser -> bridge server -> ankro -> target`

## Run

1. Start the target command:

```bash
./example-app/target
```

2. Start ankro and point it at the target:

```bash
cargo run -- serve --port 1234 --target ./example-app/target --ban-threshold 1000
```

3. Start the bridge server that the browser talks to:

```bash
node example-app/server.mjs
```

4. Open `http://127.0.0.1:3000/`

## Environment

- `EXAMPLE_APP_PORT` sets the bridge port, default `3000`
- `ANKRO_HOST` sets the ankro host, default `127.0.0.1`
- `ANKRO_PORT` sets the ankro port, default `1234`
- `ANKRO_EXAMPLE_LATENCY_MS` sets target latency, default `240`

## Test

```bash
node --test example-app/protocol.test.mjs example-app/target.test.mjs
```
