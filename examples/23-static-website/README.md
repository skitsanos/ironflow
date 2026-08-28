# Static website

Run the example from the repository root:

```bash
cargo run -- -C examples/23-static-website/ironflow.yaml serve
```

Then open <http://127.0.0.1:3101/>. The page loads its stylesheet and script as
separate static assets, and the script reads IronFlow's public `/health`
endpoint from the same origin. Stop the server with Ctrl-C.

The static files are public. Configuring an API key still protects the reserved
workflow and operator routes according to the normal `ironflow serve` contract.
