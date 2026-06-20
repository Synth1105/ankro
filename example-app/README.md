# Example App

This is a static mock DDoS queue lab.

## Run

Serve the folder with any static server, for example:

```bash
python3 -m http.server 8000 -d example-app
```

Then open `http://127.0.0.1:8000/`.

## Test

```bash
node --test example-app/sim.test.mjs
```
