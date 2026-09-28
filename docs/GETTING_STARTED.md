# Getting started with IronFlow

Use a prebuilt binary to run Lua workflows directly, or Docker to start the API
without installing IronFlow on your machine. Both include the Lua runtime.
The [README Quick Start](../README.md#quick-start) walks through your first flow
and API request.

## Download a binary

Open the [latest stable release](https://github.com/skitsanos/ironflow/releases/latest)
and expand **Assets**. Download the archive matching your system and the
`checksums-sha256.txt` file from the same release. Asset names include the
release version; `<version>` below stands for that version, such as `1.18.1`.

| System | Default archive |
| --- | --- |
| Linux x86-64 | `ironflow-v<version>-x86_64-unknown-linux-musl.tar.gz` |
| macOS Apple Silicon | `ironflow-v<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `ironflow-v<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86-64 | `ironflow-v<version>-x86_64-pc-windows-msvc.zip` |

Choose the default archive for local JSON or SQLite state storage. For
PostgreSQL or Redis storage, choose the matching archive with `-full` before
`.tar.gz` or `.zip`. The full variant enables both storage backends; it does
not bundle database servers. Linux ARM64 and Windows ARM64 native binaries
are not currently published.

No Rust compiler or separate Lua installation is required. Some nodes need
additional tools or libraries: for example, PDF image rendering needs native
Pdfium even with a full binary or the Docker image. See the
[example requirements](../examples/README.md#requirements-and-effects).

### Linux and macOS

In the directory containing the downloaded archive, verify its SHA-256 hash
against its entry in `checksums-sha256.txt`:

```bash
# Linux; replace the filename with the asset you downloaded.
sha256sum ironflow-v1.18.1-x86_64-unknown-linux-musl.tar.gz

# macOS Apple Silicon; use x86_64-apple-darwin for an Intel Mac.
shasum -a 256 ironflow-v1.18.1-aarch64-apple-darwin.tar.gz
```

Only continue if the hash matches. Extract your archive with `tar -xzf`, for
example on Linux:

```bash
tar -xzf ironflow-v1.18.1-x86_64-unknown-linux-musl.tar.gz
./ironflow --version
```

You can keep using `./ironflow` in that directory. To make `ironflow` available
elsewhere without `sudo`:

```bash
mkdir -p "$HOME/.local/bin"
install -m 755 ironflow "$HOME/.local/bin/ironflow"
export PATH="$HOME/.local/bin:$PATH"
ironflow --version
```

Add the `export PATH` line to your shell startup file (for example `~/.zshrc`
or `~/.bashrc`) to keep it in future terminals. If macOS blocks a downloaded
binary, verify the checksum and use **System Settings → Privacy & Security**
to approve that binary.

### Windows

In PowerShell, verify the archive hash against its entry in
`checksums-sha256.txt`, then extract it. Substitute the version you downloaded:

```powershell
Get-FileHash .\ironflow-v1.18.1-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

After confirming that the hash matches:

```powershell
Expand-Archive .\ironflow-v1.18.1-x86_64-pc-windows-msvc.zip -DestinationPath .\ironflow
Set-Location .\ironflow
.\ironflow.exe --version
```

Use `.\ironflow.exe validate hello.lua` and `.\ironflow.exe run hello.lua`
with the workflow from the [Quick Start](../README.md#download-and-run-a-binary).
Optionally move `ironflow.exe` to a permanent directory and add that directory
to your user `Path` through **Environment Variables**.

## Use Docker

The public image is published to
[GitHub Container Registry](https://github.com/skitsanos/ironflow/pkgs/container/ironflow).
Docker Desktop or Docker Engine must be running. Images include PostgreSQL
and Redis storage support and currently target **linux/amd64** only. On ARM
hosts, Docker must support x86-64 emulation; on Windows, use Linux containers.

### Select an image

Images are published from `main` and `develop` for build-related changes and
manual Container workflow runs:

| Tag or reference | Use |
| --- | --- |
| `ghcr.io/skitsanos/ironflow:latest` | Most recently published build from `main`; the Quick Start default. |
| `ghcr.io/skitsanos/ironflow:develop` | Most recently published build from `develop`; may include unreleased changes. |
| `ghcr.io/skitsanos/ironflow:sha-<full-commit>` | A specific published source revision. |
| `ghcr.io/skitsanos/ironflow@sha256:...` | An exact image digest for reproducible deployment. |

In Bash or Zsh, select an image once for the commands below:

```bash
IMAGE=ghcr.io/skitsanos/ironflow:latest
docker pull --platform linux/amd64 "$IMAGE"
docker run --rm --platform linux/amd64 "$IMAGE" --version
```

Use `IMAGE=ghcr.io/skitsanos/ironflow:develop` to try development builds.
`latest` follows `main`, not the GitHub Releases API; check `--version` when
matching an image to a binary release. Moving tags update only after a
successful Container workflow run on their respective branch. Pull again
before restarting to use an updated image, or use `docker run --pull always`
as shown below. Running containers do not update automatically.

Find published commit tags on the
[package page](https://github.com/skitsanos/ironflow/pkgs/container/ironflow),
or open a successful [Container workflow run](https://github.com/skitsanos/ironflow/actions/workflows/container.yml)
and copy the `ghcr.io/skitsanos/ironflow@sha256:...` reference from its
**Published container** summary into `IMAGE`. A source commit alone does not
guarantee that its image has finished publishing. If an alias is not available
yet, use a published commit tag or digest. The `buildcache-amd64` tag is build
cache, not an application image. Hosted deployments use the
[digest-pinned deployment procedure](REPLICA_DEPLOYMENT.md).

### Start a local API server

```bash
docker run --rm --pull always --name ironflow --platform linux/amd64 \
  -p 127.0.0.1:3000:3000 \
  -e IRONFLOW_ALLOW_UNAUTHENTICATED_API=true \
  -e IRONFLOW_STORE_DIR=/data/state/runs \
  -v ironflow-data:/data \
  "$IMAGE"
```

The image starts `ironflow serve` automatically. It listens on `0.0.0.0:3000`
inside the container; the port mapping exposes it only on your host's
`127.0.0.1:3000`. The explicit unauthenticated setting is for this local demo.
For a hosted server, set `IRONFLOW_API_KEY` and follow the
[authentication reference](CLI_REFERENCE.md#api-authentication).

In a second terminal, run `curl --fail http://127.0.0.1:3000/health/ready`, then
try the [Quick Start API request](../README.md#try-the-rest-api).
Stop the server with **Ctrl+C**, or run `docker stop ironflow` in another
terminal. The named `ironflow-data` volume keeps files under `/data`, including
run history at `/data/state/runs`, when the container is removed. Reuse the same
volume and store-directory setting on your next run. The explicit path also
works with older images that pre-created `/data/runs` as root. New images let
the runtime UID create the default `/data/runs` with private `0700` permissions.
Remove the volume with `docker volume rm ironflow-data` only when you want to
delete that local demo's stored data.

Updating the image does not repair an existing root-owned store directory.
Stop all writers and back up the volume before an operator corrects ownership
for the intended runtime UID, or select a new runtime-owned directory and
migrate the retained state offline. Do not loosen the store's `0700` directory
or `0600` file permissions. Reusing a JSON volume requires the same owning UID;
arbitrary-UID containers can initialize fresh volumes, while replicas should
use the shared storage described in the [deployment guide](REPLICA_DEPLOYMENT.md).

### Run a workflow file

Save the Quick Start's `hello.lua` in your current directory. Mount that
directory read-only and use the CLI directly:

```bash
docker run --rm --platform linux/amd64 \
  --mount "type=bind,source=$(pwd),target=/data/flows,readonly" \
  "$IMAGE" validate /data/flows/hello.lua

docker run --rm --platform linux/amd64 \
  --mount "type=bind,source=$(pwd),target=/data/flows,readonly" \
  -v ironflow-data:/data \
  -e IRONFLOW_STORE_DIR=/data/state/runs \
  "$IMAGE" run /data/flows/hello.lua
```

Paths inside workflows are container paths. The flow mount above is read-only;
use a writable path under `/data` for outputs. These commands use Bash/Zsh
syntax; for PowerShell, set `$IMAGE = 'ghcr.io/skitsanos/ironflow:latest'`,
use `$PWD.Path` for the host directory, and put each Docker command on one
line instead of using backslash continuations.

## Build from source

Building is optional. For development or platforms without a published
binary, install Rust with rustup plus the native compiler/linker tools for
your platform. The repository's `rust-toolchain.toml` selects the exact Rust
version used by the build.

```bash
git clone https://github.com/skitsanos/ironflow.git
cd ironflow
cargo build --locked --release
./target/release/ironflow --version
./target/release/ironflow run examples/01-basics/hello_world.lua --context '{"user_name": "Alice"}'
```

To include PostgreSQL and Redis storage support, build with
`cargo build --locked --release --features postgres,redis`.
On Windows, the executable is `target\release\ironflow.exe`.
To build a specific stable release, check out its tag (for example
`git checkout v1.18.1`) before running Cargo.

## Next steps

- [Examples](../examples/README.md): workflows grouped from basics to advanced use.
- [CLI reference](CLI_REFERENCE.md): configuration, commands, authentication, and storage.
- [Replica deployment](REPLICA_DEPLOYMENT.md): shared storage and hosted deployment requirements.
